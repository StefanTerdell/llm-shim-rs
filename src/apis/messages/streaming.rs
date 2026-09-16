use crate::{
    apis::messages::models::{
        api::{
            request::streaming::StreamingMessagesRequestBody,
            response::streaming::MessagesStreamEvent,
        },
        lib::{
            options::{MessagesOptions, reasoning_remapping::MessagesReasoningRemappingState},
            streaming::response::{StreamingMessagesEvent, StreamingMessagesResponse},
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
use tokio_stream::StreamExt;

pub const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

pub async fn streaming_messages<'a>(
    url: impl IntoUrl,
    body: impl Into<StreamingMessagesRequestBody>,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Result<StreamingMessagesResponse<'a>, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();

    let client = options.client.unwrap_or_default();
    let tps_throttler = options.tps_throttler;
    let mut remapper = options
        .reasoning_remapping
        .map(MessagesReasoningRemappingState::new);

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
    let mut pacer = TpsThrottler::new(tps_throttler, stats.requested);
    let mut stream = sse::send(request).await?;
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            let Some(event) = frames.push::<MessagesStreamEvent>(data)? else {
                continue;
            };

            match &event {
                MessagesStreamEvent::MessageStart { message, .. } => {
                    if let Some(input_tokens) = message.usage.input_tokens {
                        stats.input_tokens_is_estimate = false;
                        stats.input_tokens = input_tokens;
                    }
                }
                MessagesStreamEvent::ContentBlockStart { content_block, .. } => {
                    let tokens = content_block.estimate_tokens();
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                MessagesStreamEvent::ContentBlockDelta { delta, .. } => {
                    let tokens = delta.estimate_tokens();
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                MessagesStreamEvent::MessageDelta { usage: Some(usage), .. } => {
                    if let Some(input_tokens) = usage.input_tokens {
                        stats.input_tokens_is_estimate = false;
                        stats.input_tokens = input_tokens;
                    }

                    if let Some(output_tokens) = usage.output_tokens {
                        stats.output_tokens_is_estimate = false;
                        stats.output_tokens = output_tokens;
                    }
                }
                MessagesStreamEvent::Error { error, .. } => {
                    stats.error = Some(error.to_string());

                    if let Some(remapper) = &mut remapper {
                        for event in remapper.flush() {
                            yield StreamingMessagesEvent::Event { event };
                        }
                    }

                    yield StreamingMessagesEvent::EventError { event, stats };
                    return;
                }
                _ => {}
            }

            let is_stop = matches!(event, MessagesStreamEvent::MessageStop { .. });

            match &mut remapper {
                Some(remapper) => {
                    for event in remapper.apply(event) {
                        yield StreamingMessagesEvent::Event { event };
                    }
                }
                None => yield StreamingMessagesEvent::Event { event },
            }

            if is_stop {
                break;
            }
        }

        if let Some(remapper) = &mut remapper {
            for event in remapper.flush() {
                yield StreamingMessagesEvent::Event { event };
            }
        }

        yield StreamingMessagesEvent::Done { stats };
    }))
}
