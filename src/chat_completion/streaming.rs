use crate::{
    chat_completion::models::{
        api::{
            request::streaming::{
                StreamingChatCompletionRequestBody, StreamingChatCompletionRequestBodyStreamOptions,
            },
            response::streaming::StreamingChatCompletionChunk,
        },
        lib::{
            common::stats::{ChatCompletionChunkStats, ChatCompletionStats},
            options::ChatCompletionOptions,
            streaming::response::{StreamingChatCompletionEvent, StreamingChatCompletionResponse},
        },
    },
    error::Error,
    traits::{estimate_tokens::EstimateTokens, tps_throttler::TpsThrottler},
};

use async_stream::try_stream;
use reqwest::IntoUrl;
use reqwest_sse::{Event, EventSource};
use std::time::{Duration, Instant};
use stefans_utils::prelude::AsBool;
use tokio::time::sleep;
use tokio_stream::StreamExt;

pub async fn streaming_chat_completion(
    url: impl IntoUrl,
    body: impl Into<StreamingChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions>>,
) -> Result<StreamingChatCompletionResponse, Error> {
    let mut body = body.into();

    let (client, bearer_token, tps_throttler, mut reasoning_content_remapping_state) =
        match options.into() {
            Some(options) => (
                options.client.unwrap_or_default(),
                options.bearer_token,
                options.tps_throttler,
                options
                    .reasoning_content_remapping
                    .map(|rcr| rcr.into_state()),
            ),
            None => (Default::default(), None, None, None),
        };

    let requested_logprobs = body.common.logprobs.as_bool();
    let requested_usage = body
        .stream_options
        .as_ref()
        .is_some_and(|so| so.include_usage.as_bool());

    body.common.logprobs = Some(true);

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

    let mut stats = ChatCompletionStats {
        requested: Instant::now(),
        chunks: Default::default(),
        input_tokens_is_estimate: true,
        input_tokens: body.common.estimate_tokens(),
        output_tokens_is_estimate: true,
        output_tokens: 0,
        error: None,
    };

    let mut instant_of_last_received_chunk = stats.requested;
    let mut stream = request.send().await?.events().await?;
    let mut partial_buffer = None;
    let mut tps_correction_secs = 0.0;

    Ok(Box::pin(try_stream! {
        while let Some(Event { mut data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                yield StreamingChatCompletionEvent::Done { stats };
                break;
            }

            if let Some(previous) = partial_buffer.take() {
                data = format!("{previous}{data}");
            }

            let Ok(mut chunk) =
                serde_json::from_str::<StreamingChatCompletionChunk>(&data)
            else {
                partial_buffer = Some(data);
                continue;
            };

            if let Some(choices) = &mut chunk.choices {
                let now = Instant::now();
                let mut chunk_duration = now - instant_of_last_received_chunk;
                instant_of_last_received_chunk = now;

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
                        stats.output_tokens_is_estimate = true;
                        chunk_tokens += choice.delta.estimate_tokens();
                    }

                    if let Some(rcr_state) = &mut reasoning_content_remapping_state {
                        rcr_state.apply(choice);
                    }
                }


                if let Some(max_tps) = tps_throttler.get_max_tps().await
                    && let chunk_tokens = chunk_tokens as f32
                    && let chunk_secs = chunk_duration.as_secs_f32()
                    && chunk_secs > 0.0
                    && chunk_tokens / chunk_secs > max_tps {
                        tps_correction_secs += chunk_tokens / max_tps - chunk_secs;
                }


                let chunk_tps_correction = if tps_correction_secs > 0.0 {
                    let tps_correction_duration = Duration::from_secs_f32(tps_correction_secs);
                    sleep(tps_correction_duration).await;
                    chunk_duration += tps_correction_duration;

                    tps_correction_secs = 0.0;

                    Some(tps_correction_duration)
                } else {
                    None
                };

                let chunk_stats = ChatCompletionChunkStats { duration: chunk_duration, tokens: chunk_tokens, tps_correction_duration: chunk_tps_correction };

                stats.chunks
                    .push(chunk_stats)
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
