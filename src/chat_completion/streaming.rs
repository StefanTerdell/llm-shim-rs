use crate::{
    chat_completion::models::{
        api::{
            request::streaming::{
                StreamingChatCompletionRequestBody, StreamingChatCompletionRequestBodyStreamOptions,
            },
            response::streaming::StreamingChatCompletionChunk,
        },
        lib::{
            options::{ChatCompletionOptions, OutputTokenCounting},
            streaming::response::{StreamingChatCompletionEvent, StreamingChatCompletionResponse},
        },
    },
    error::Error,
    sse::{self, Event, JsonFrameBuffer},
    stats::StreamStats,
    tps_throttler::TpsThrottler,
    traits::estimate_tokens::EstimateTokens,
};

use async_stream::try_stream;
use reqwest::IntoUrl;
use std::time::Instant;
use stefans_utils::prelude::AsBool;
use tokio_stream::StreamExt;

pub async fn streaming_chat_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<StreamingChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions<'a>>>,
) -> Result<StreamingChatCompletionResponse<'a>, Error> {
    let mut body = body.into();

    let (
        client,
        bearer_token,
        max_tps,
        mut reasoning_content_remapping_state,
        output_token_counting,
    ) = match options.into() {
        Some(options) => (
            options.client.unwrap_or_default(),
            options.bearer_token,
            options.max_tps,
            options
                .reasoning_content_remapping
                .map(|rcr| rcr.into_state()),
            options.output_token_counting,
        ),
        None => (
            Default::default(),
            None,
            None,
            None,
            OutputTokenCounting::default(),
        ),
    };

    let requested_logprobs = body.common.logprobs.as_bool();
    let requested_usage = body
        .stream_options
        .as_ref()
        .is_some_and(|so| so.include_usage.as_bool());

    if output_token_counting == OutputTokenCounting::Logprobs {
        body.common.logprobs = Some(true);
    }

    match body.stream_options.as_mut() {
        Some(stream_options) => {
            stream_options.include_usage = Some(true);
        }
        None => {
            body.stream_options = Some(StreamingChatCompletionRequestBodyStreamOptions {
                include_usage: Some(true),
                additional_properties: Default::default(),
            });
        }
    }

    let mut request = client.post(url).json(&body);

    if let Some(secret) = bearer_token {
        request = request.bearer_auth(secret.expose())
    };

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());
    let mut pacer = TpsThrottler::new(max_tps, stats.requested);
    let mut stream = sse::send(request).await?;
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                yield StreamingChatCompletionEvent::Done { stats };
                break;
            }

            let Some(mut chunk) = frames.push::<StreamingChatCompletionChunk>(data)? else {
                continue;
            };

            if let Some(choices) = &mut chunk.choices {
                let mut chunk_tokens = 0;

                for choice in choices {
                    if let Some(logprobs) = choice.common.logprobs.take() {
                        let count: u32 = logprobs.values().map(|v| v.len() as u32).sum();
                        stats.output_tokens_is_estimate = false;
                        stats.output_tokens += count;
                        chunk_tokens += count;

                        if requested_logprobs {
                            choice.common.logprobs = Some(logprobs);
                        }
                    } else {
                        let count = choice.delta.estimate_tokens();
                        stats.output_tokens_is_estimate = true;
                        stats.output_tokens += count;
                        chunk_tokens += count;
                    }

                    if let Some(rcr_state) = &mut reasoning_content_remapping_state {
                        rcr_state.apply(choice);
                    }
                }

                stats.chunks.push(pacer.pace(chunk_tokens).await);
            }

            if let Some(usage) = chunk.usage.take() {
                stats.input_tokens_is_estimate = false;
                stats.input_tokens = usage.prompt_tokens();
                stats.output_tokens_is_estimate = false;
                stats.output_tokens = usage.completion_tokens();

                if requested_usage {
                    chunk.usage = Some(usage);
                }
            }

            if let Some(error) = chunk.error.as_ref().map(|e| e.to_string()).filter(|e| !e.is_empty()) {
                stats.error = Some(error);
                yield StreamingChatCompletionEvent::ChunkError { chunk, stats };
                break;
            }

            yield StreamingChatCompletionEvent::Chunk { chunk };
        }
    }))
}
