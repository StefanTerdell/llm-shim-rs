use crate::{
    apis::messages::{
        models::{
            api::{
                request::non_streaming::NonStreamingMessagesRequestBody,
                response::non_streaming::{MessagesResponseBody, NonStreamingMessagesResponse},
            },
            lib::options::{MessagesOptions, reasoning_remapping::body::remap_content},
        },
        streaming::DEFAULT_ANTHROPIC_VERSION,
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub async fn non_streaming_messages<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingMessagesRequestBody>,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Result<NonStreamingMessagesResponse, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();

    let mut request = client.post(url).json(&body).header(
        "anthropic-version",
        options
            .anthropic_version
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_VERSION.to_string()),
    );

    if let Some(api_key) = options.api_key {
        request = request.header("x-api-key", api_key.expose());
    }

    if let Some(bearer_token) = options.bearer_token {
        request = request.bearer_auth(bearer_token.expose());
    }

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());

    let (mut body, chunk) = pace_once(
        options.max_tps,
        options.max_tps_sample_interval,
        async {
            Ok::<_, Error>(
                request
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<MessagesResponseBody>()
                    .await?,
            )
        },
        output_tokens,
    )
    .await?;

    if let Some(input_tokens) = body.usage.input_tokens {
        stats.input_tokens_is_estimate = false;
        stats.input_tokens = input_tokens;
    }

    match body.usage.output_tokens {
        Some(output_tokens) => {
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = output_tokens;
        }
        None => stats.output_tokens = output_tokens(&body),
    }

    stats.chunks.push(chunk);

    if let Some(config) = &options.reasoning_remapping {
        body.content = remap_content(std::mem::take(&mut body.content), config);
    }

    let error = body
        .additional_properties
        .get("error")
        .filter(|error| !error.is_null())
        .cloned();

    if let Some(error) = &error {
        stats.error = Some(error.to_string());
    }

    body.usage.input_tokens.get_or_insert(stats.input_tokens);
    body.usage.output_tokens.get_or_insert(stats.output_tokens);

    Ok(NonStreamingMessagesResponse {
        body,
        stats: Some(stats),
        error,
    })
}

fn output_tokens(body: &MessagesResponseBody) -> u32 {
    body.usage.output_tokens.unwrap_or_else(|| {
        body.content
            .iter()
            .map(EstimateTokens::estimate_tokens)
            .sum()
    })
}
