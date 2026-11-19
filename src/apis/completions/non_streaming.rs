use crate::{
    apis::completions::models::{
        api::{
            request::NonStreamingCompletionRequestBody,
            response::{NonStreamingCompletionResponse, NonStreamingCompletionResponseBody},
        },
        lib::options::CompletionsOptions,
    },
    error::{Error, send_checked},
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub async fn non_streaming_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingCompletionRequestBody>,
    options: impl Into<Option<CompletionsOptions<'a>>>,
) -> Result<NonStreamingCompletionResponse, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();

    let mut request = options.client.unwrap_or_default().post(url).json(&body);

    if let Some(credentials) = options.credentials {
        request = credentials.apply(request);
    }

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());

    let (mut body, chunk) = pace_once(
        options.max_tps,
        options.max_tps_sample_interval,
        async {
            Ok::<_, Error>(
                send_checked(request)
                    .await?
                    .json::<NonStreamingCompletionResponseBody>()
                    .await?,
            )
        },
        output_tokens,
    )
    .await?;

    match &body.usage {
        Some(usage) => {
            stats.input_tokens_is_estimate = false;
            stats.input_tokens = usage.prompt_tokens();
            stats.cached_input_tokens = usage.cached_tokens();
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = usage.completion_tokens();
        }
        None => stats.output_tokens = output_tokens(&body),
    }

    stats.chunks.push(chunk);

    let error = body.error.take();

    if let Some(error) = &error {
        stats.error = Some(error.to_string());
    }

    body.usage.get_or_insert_with(|| (&stats).into());

    Ok(NonStreamingCompletionResponse {
        body,
        stats: Some(stats),
        error,
    })
}

fn output_tokens(body: &NonStreamingCompletionResponseBody) -> u32 {
    match &body.usage {
        Some(usage) => usage.completion_tokens(),
        None => body
            .choices
            .iter()
            .map(|choice| choice.text.estimate_tokens())
            .sum(),
    }
}
