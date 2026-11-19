use crate::{
    apis::completions::models::{
        api::{
            request::{
                StreamingCompletionRequestBody, StreamingCompletionRequestBodyStreamOptions,
            },
            response::StreamingCompletionChunk,
        },
        lib::{
            options::CompletionsOptions,
            streaming::response::{StreamingCompletionEvent, StreamingCompletionResponse},
        },
    },
    error::Error,
    sse::{self, Event, JsonFrameBuffer},
    stats::StreamStats,
    tps_throttler::TpsThrottler,
    traits::estimate_tokens::EstimateTokens,
};

use async_stream::try_stream;
use std::time::Instant;
use stefans_utils::prelude::AsBool;
use tokio_stream::StreamExt;

pub async fn streaming_completion<'a>(
    url: impl reqwest::IntoUrl,
    body: impl Into<StreamingCompletionRequestBody>,
    options: impl Into<Option<CompletionsOptions<'a>>>,
) -> Result<StreamingCompletionResponse<'a>, Error> {
    let mut body = body.into();
    let options = options.into().unwrap_or_default();

    let requested_usage = body
        .stream_options
        .as_ref()
        .is_some_and(|so| so.include_usage.as_bool());

    match body.stream_options.as_mut() {
        Some(stream_options) => stream_options.include_usage = Some(true),
        None => {
            body.stream_options = Some(StreamingCompletionRequestBodyStreamOptions {
                include_usage: Some(true),
                additional_properties: Default::default(),
            });
        }
    }

    let mut request = options.client.unwrap_or_default().post(url).json(&body);

    if let Some(credentials) = options.credentials {
        request = credentials.apply(request);
    }

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());
    let mut pacer = TpsThrottler::new(options.max_tps, stats.requested);
    let mut stream = sse::send(request).await?;
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                yield StreamingCompletionEvent::Done { stats };
                return;
            }

            let Some(mut chunk) = frames.push::<StreamingCompletionChunk>(data)? else {
                continue;
            };

            if let Some(choices) = &chunk.choices {
                let chunk_tokens = choices.iter().map(|choice| choice.text.estimate_tokens()).sum();
                if chunk_tokens > 0 {
                    stats.output_tokens_is_estimate = true;
                    stats.output_tokens += chunk_tokens;
                }
                stats.chunks.push(pacer.pace(chunk_tokens).await);
            }

            if let Some(usage) = chunk.usage.take() {
                stats.input_tokens_is_estimate = false;
                stats.input_tokens = usage.prompt_tokens();
                stats.cached_input_tokens = usage.cached_tokens();
                stats.output_tokens_is_estimate = false;
                stats.output_tokens = usage.completion_tokens();

                if requested_usage {
                    chunk.usage = Some(usage);
                }
            }

            if let Some(error) = chunk.error.as_ref().map(|e| e.to_string()).filter(|e| !e.is_empty()) {
                stats.error = Some(error);
                yield StreamingCompletionEvent::ChunkError { chunk, stats };
                return;
            }

            yield StreamingCompletionEvent::Chunk { chunk };
        }

        yield StreamingCompletionEvent::Done { stats };
    }))
}
