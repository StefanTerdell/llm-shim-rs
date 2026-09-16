use crate::{
    apis::rerank::models::{
        api::{
            request::RerankRequestBody,
            response::{RerankResponse, RerankResponseBody},
        },
        lib::options::RerankOptions,
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub mod models;

pub async fn rerank<'a>(
    url: impl IntoUrl,
    body: impl Into<RerankRequestBody>,
    options: impl Into<Option<RerankOptions<'a>>>,
) -> Result<RerankResponse, Error> {
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
                    .json::<RerankResponseBody>()
                    .await?,
            )
        },
        |body| {
            body.usage
                .as_ref()
                .and_then(|u| u.total_tokens.or(u.prompt_tokens))
                .unwrap_or(input_tokens_estimate)
        },
    )
    .await?;

    if let Some(tokens) = body
        .usage
        .as_ref()
        .and_then(|u| u.total_tokens.or(u.prompt_tokens))
    {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = tokens;
    }

    stats.chunks.push(chunk);

    Ok(RerankResponse { body, stats })
}
