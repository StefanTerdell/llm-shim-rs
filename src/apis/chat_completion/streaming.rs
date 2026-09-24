use crate::{
    apis::chat_completion::models::{
        api::{
            request::streaming::{
                StreamingChatCompletionRequestBody, StreamingChatCompletionRequestBodyStreamOptions,
            },
            response::{
                common::{ChatCompletionResponseMessage, CommonChatCompletionChoice},
                streaming::{StreamingChatCompletionChoice, StreamingChatCompletionChunk},
            },
        },
        lib::{
            options::{
                ChatCompletionOptions, OutputTokenCounting,
                reasoning_remapping::ChatCompletionReasoningRemappingState,
            },
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
use indexmap::IndexMap;
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

    let (client, bearer_token, max_tps, mut reasoning_remapping_state, output_token_counting) =
        match options.into() {
            Some(options) => (
                options.client.unwrap_or_default(),
                options.bearer_token,
                options.max_tps,
                options
                    .reasoning_remapping
                    .map(|config| config.into_state()),
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
        let mut last_metadata = IndexMap::new();

        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                if let Some(state) = &mut reasoning_remapping_state
                    && let Some(chunk) = closing_chunk(state, last_metadata)
                {
                    yield StreamingChatCompletionEvent::Chunk { chunk };
                }

                yield StreamingChatCompletionEvent::Done { stats };
                break;
            }

            let Some(mut chunk) = frames.push::<StreamingChatCompletionChunk>(data)? else {
                continue;
            };

            last_metadata = chunk.additional_properties.clone();

            if let Some(choices) = &mut chunk.choices {
                let mut chunk_tokens = 0;

                for choice in choices {
                    if let Some(logprobs) = choice.common.logprobs.take() {
                        let count: u32 = logprobs.values().flatten().map(|v| v.len() as u32).sum();
                        stats.output_tokens_is_estimate = false;
                        stats.output_tokens += count;
                        chunk_tokens += count;

                        if requested_logprobs {
                            choice.common.logprobs = Some(logprobs);
                        }
                    } else {
                        let count = choice.delta.estimate_tokens();
                        if count > 0 {
                            stats.output_tokens_is_estimate = true;
                            stats.output_tokens += count;
                            chunk_tokens += count;
                        }
                    }

                    if let Some(state) = &mut reasoning_remapping_state {
                        state.apply(choice.common.index, &mut choice.delta);
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

/// Closes any reasoning section the stream left open, as one extra chunk that
/// reuses the stream's metadata (id, model, created, ...).
fn closing_chunk(
    state: &mut ChatCompletionReasoningRemappingState,
    metadata: IndexMap<String, serde_json::Value>,
) -> Option<StreamingChatCompletionChunk> {
    let choices: Vec<StreamingChatCompletionChoice> = state
        .open_indices()
        .into_iter()
        .filter_map(|index| {
            let mut delta = ChatCompletionResponseMessage {
                content: None,
                common: Default::default(),
            };
            state.flush(index, &mut delta);

            let is_empty = delta.content().is_none()
                && delta.common.reasoning().is_none()
                && delta.common.reasoning_content().is_none();

            (!is_empty).then_some(StreamingChatCompletionChoice {
                delta,
                common: CommonChatCompletionChoice {
                    index,
                    logprobs: None,
                    additional_properties: Default::default(),
                },
            })
        })
        .collect();

    (!choices.is_empty()).then_some(StreamingChatCompletionChunk {
        error: None,
        choices: Some(choices),
        usage: None,
        additional_properties: metadata,
    })
}
