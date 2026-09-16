use crate::{
    embeddings::models::{
        api::{
            request::EmbeddingsRequestBody,
            response::{EmbeddingsResponse, EmbeddingsResponseBody},
        },
        lib::options::EmbeddingsOptions,
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub mod models;

pub async fn embeddings<'a>(
    url: impl IntoUrl,
    body: impl Into<EmbeddingsRequestBody>,
    options: impl Into<Option<EmbeddingsOptions<'a>>>,
) -> Result<EmbeddingsResponse, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();

    let mut request = client.post(url).json(&body);

    if let Some(bearer_token) = options.bearer_token {
        request = request.bearer_auth(bearer_token.expose());
    }

    let input_tokens_estimate = body.estimate_tokens();
    let mut stats = StreamStats::new(Instant::now(), input_tokens_estimate);
    stats.output_tokens_is_estimate = false;

    let (body, chunk) = pace_once(
        options.tps_throttler,
        async {
            Ok::<_, Error>(
                request
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<EmbeddingsResponseBody>()
                    .await?,
            )
        },
        |body| {
            body.usage
                .as_ref()
                .and_then(|u| u.prompt_tokens.or(u.total_tokens))
                .unwrap_or(input_tokens_estimate)
        },
    )
    .await?;

    if let Some(tokens) = body
        .usage
        .as_ref()
        .and_then(|u| u.prompt_tokens.or(u.total_tokens))
    {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = tokens;
    }

    stats.chunks.push(chunk);

    Ok(EmbeddingsResponse { body, stats })
}
