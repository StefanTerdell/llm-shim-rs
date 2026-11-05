use crate::{
    apis::systemone::models::{
        api::{
            request::SystemOneRequestBody,
            response::{SystemOneResponse, SystemOneResponseBody},
        },
        lib::options::SystemOneOptions,
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub mod models;

/// `POST /v1/systemone` (Jev / Laya typed decisions). Nothing is generated,
/// so all tokens are input tokens and the output count is always exact.
pub async fn systemone<'a>(
    url: impl IntoUrl,
    body: impl Into<SystemOneRequestBody>,
    options: impl Into<Option<SystemOneOptions<'a>>>,
) -> Result<SystemOneResponse, Error> {
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
        options.max_tps,
        options.max_tps_sample_interval,
        async {
            Ok::<_, Error>(
                request
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<SystemOneResponseBody>()
                    .await?,
            )
        },
        |body| {
            body.usage
                .as_ref()
                .and_then(|u| u.input_tokens)
                .unwrap_or(input_tokens_estimate)
        },
    )
    .await?;

    if let Some(tokens) = body.usage.as_ref().and_then(|u| u.input_tokens) {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = tokens;
    }

    stats.chunks.push(chunk);

    Ok(SystemOneResponse { body, stats })
}
