use crate::{
    apis::chat_completion::models::{
        api::{
            request::non_streaming::NonStreamingChatCompletionRequestBody,
            response::non_streaming::{
                NonStreamingChatCompletionResponse, NonStreamingChatCompletionResponseBody,
            },
        },
        lib::options::ChatCompletionOptions,
    },
    error::Error,
    stats::StreamStats,
    tps_throttler::pace_once,
    traits::estimate_tokens::EstimateTokens,
};

use reqwest::IntoUrl;
use std::time::Instant;

pub async fn non_streaming_chat_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions<'a>>>,
) -> Result<NonStreamingChatCompletionResponse, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();
    let mut remapper = options
        .reasoning_remapping
        .map(|config| config.into_state());

    let mut request = client.post(url).json(&body);

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
                    .json::<NonStreamingChatCompletionResponseBody>()
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
            stats.output_tokens_is_estimate = false;
            stats.output_tokens = usage.completion_tokens();
        }
        None => stats.output_tokens = output_tokens(&body),
    }

    stats.chunks.push(chunk);

    if let Some(remapper) = &mut remapper {
        for choice in &mut body.choices {
            remapper.apply(choice.common.index, &mut choice.message);
            remapper.flush(choice.common.index, &mut choice.message);
        }
    }

    let error = body.error.take();

    if let Some(error) = &error {
        stats.error = Some(error.to_string());
    }

    body.usage.get_or_insert_with(|| (&stats).into());

    Ok(NonStreamingChatCompletionResponse {
        body,
        stats: Some(stats),
        error,
    })
}

fn output_tokens(body: &NonStreamingChatCompletionResponseBody) -> u32 {
    match &body.usage {
        Some(usage) => usage.completion_tokens(),
        None => body
            .choices
            .iter()
            .map(|choice| choice.message.estimate_tokens())
            .sum(),
    }
}
