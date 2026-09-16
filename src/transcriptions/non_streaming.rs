use crate::{
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
    transcriptions::models::{
        api::{
            request::TranscriptionsRequest,
            response::non_streaming::{
                NonStreamingTranscriptionsResponse, TranscriptionsJsonBody,
                TranscriptionsResponseBody,
            },
        },
        lib::options::TranscriptionsOptions,
    },
};

use reqwest::{IntoUrl, header::CONTENT_TYPE};
use std::time::Instant;

pub async fn non_streaming_transcriptions<'a>(
    url: impl IntoUrl,
    request: TranscriptionsRequest,
    options: impl Into<Option<TranscriptionsOptions<'a>>>,
) -> Result<NonStreamingTranscriptionsResponse, Error> {
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();

    let mut builder = client.post(url).multipart(request.into_form(None)?);

    if let Some(bearer_token) = options.bearer_token {
        builder = builder.bearer_auth(bearer_token.expose());
    }

    let mut stats = StreamStats::new(Instant::now(), 0);

    let (body, chunk) = pace_once(
        options.tps_throttler,
        async {
            let response = builder.send().await?.error_for_status()?;
            let is_json = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.trim_start().starts_with("application/json"));
            let text = response.text().await?;

            Ok::<_, Error>(if is_json {
                TranscriptionsResponseBody::Json(serde_json::from_str::<TranscriptionsJsonBody>(
                    &text,
                )?)
            } else {
                TranscriptionsResponseBody::Text(text)
            })
        },
        |body| {
            body.usage()
                .and_then(|u| u.output_tokens)
                .unwrap_or_else(|| body.text().estimate_tokens())
        },
    )
    .await?;

    if let Some(input_tokens) = body.usage().and_then(|u| u.input_tokens) {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = input_tokens;
    }

    match body.usage().and_then(|u| u.output_tokens) {
        Some(output_tokens) => {
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = output_tokens;
        }
        None => stats.output_tokens = body.text().estimate_tokens(),
    }

    stats.chunks.push(chunk);

    Ok(NonStreamingTranscriptionsResponse { body, stats })
}
