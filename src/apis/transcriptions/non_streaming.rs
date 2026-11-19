use crate::{
    apis::transcriptions::models::{
        api::{
            request::{TranscriptionsRequest, TranscriptionsResponseFormat},
            response::non_streaming::{
                NonStreamingTranscriptionsResponse, TranscriptionsJsonBody,
                TranscriptionsResponseBody,
            },
        },
        lib::{options::TranscriptionsOptions, response_format::render_verbose_json},
    },
    error::{Error, send_checked},
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::{IntoUrl, Response, header::CONTENT_TYPE};
use std::time::Instant;

pub async fn non_streaming_transcriptions<'a>(
    url: impl IntoUrl,
    request: TranscriptionsRequest,
    options: impl Into<Option<TranscriptionsOptions<'a>>>,
) -> Result<NonStreamingTranscriptionsResponse, Error> {
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();
    let response_format = request.fields.response_format;
    let upstream_format = (options.upstream_verbose_json
        && response_format.is_none_or(TranscriptionsResponseFormat::renders_from_verbose_json))
    .then_some(TranscriptionsResponseFormat::VerboseJson);

    let mut builder = client
        .post(url)
        .multipart(request.into_form(None, upstream_format)?);

    if let Some(credentials) = options.credentials {
        builder = credentials.apply(builder);
    }

    let mut stats = StreamStats::new(Instant::now(), 0);

    let (body, chunk) = pace_once(
        options.max_tps,
        options.max_tps_sample_interval,
        async { read_body(send_checked(builder).await?).await },
        |body| {
            body.usage()
                .and_then(|u| u.output_tokens)
                .unwrap_or_else(|| body.text().estimate_tokens())
        },
    )
    .await?;

    let usage = match &body {
        TranscriptionsResponseBody::Json(json) => json.usage_or_duration(),
        TranscriptionsResponseBody::Text(_) => None,
    };

    if let Some(input_tokens) = usage.as_ref().and_then(|u| u.input_tokens) {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = input_tokens;
    }

    match usage.as_ref().and_then(|u| u.output_tokens) {
        Some(output_tokens) => {
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = output_tokens;
        }
        None => stats.output_tokens = body.text().estimate_tokens(),
    }

    stats.chunks.push(chunk);

    let body = match (upstream_format, body) {
        (Some(_), TranscriptionsResponseBody::Json(json)) => render_verbose_json(
            json,
            response_format.unwrap_or(TranscriptionsResponseFormat::Json),
        ),
        (_, body) => body,
    };

    Ok(NonStreamingTranscriptionsResponse { body, usage, stats })
}

pub(crate) async fn read_body(response: Response) -> Result<TranscriptionsResponseBody, Error> {
    let is_json = has_content_type(&response, "application/json");
    let text = response.text().await?;

    Ok(if is_json {
        TranscriptionsResponseBody::Json(serde_json::from_str::<TranscriptionsJsonBody>(&text)?)
    } else {
        TranscriptionsResponseBody::Text(text)
    })
}

pub(crate) fn has_content_type(response: &Response, content_type: &str) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.trim_start().starts_with(content_type))
}
