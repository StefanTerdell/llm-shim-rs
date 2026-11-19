use crate::{
    apis::transcriptions::{
        models::{
            api::{
                request::TranscriptionsRequest,
                response::{
                    non_streaming::TranscriptionsResponseBody, streaming::TranscriptionsStreamEvent,
                },
            },
            lib::{
                options::TranscriptionsOptions,
                streaming::{
                    normalization::EventNormalizer,
                    response::{StreamingTranscriptionsEvent, StreamingTranscriptionsResponse},
                },
            },
        },
        non_streaming::{has_content_type, read_body},
    },
    error::{Error, send_checked},
    sse::{self, Event, JsonFrameBuffer},
    stats::StreamStats,
    tps_throttler::TpsThrottler,
    traits::estimate_tokens::EstimateTokens,
};

use async_stream::try_stream;
use reqwest::IntoUrl;
use serde_json::Value;
use std::time::Instant;
use tokio_stream::StreamExt;

pub async fn streaming_transcriptions<'a>(
    url: impl IntoUrl,
    request: TranscriptionsRequest,
    options: impl Into<Option<TranscriptionsOptions<'a>>>,
) -> Result<StreamingTranscriptionsResponse<'a>, Error> {
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();
    let max_tps = options.max_tps;
    let mut normalizer = options.normalize_events.then(EventNormalizer::default);

    let mut builder = client
        .post(url)
        .multipart(request.into_form(Some(true), None)?);

    if let Some(credentials) = options.credentials {
        builder = credentials.apply(builder);
    }

    let mut stats = StreamStats::new(Instant::now(), 0);
    let mut pacer = TpsThrottler::new(max_tps, stats.requested);
    let response = send_checked(builder).await?;

    if !has_content_type(&response, "text/event-stream") {
        let body = read_body(response).await?;
        let passthrough = match (&normalizer, &body) {
            (Some(_), _) => None,
            (None, TranscriptionsResponseBody::Json(json)) => Some(serde_json::to_value(json)?),
            (None, TranscriptionsResponseBody::Text(text)) => Some(Value::String(text.clone())),
        };
        let normalized = EventNormalizer::from_body(body);

        return Ok(Box::pin(try_stream! {
            for event in &normalized {
                record(event, &mut stats, &mut pacer).await;
            }

            match passthrough {
                Some(value) => {
                    yield StreamingTranscriptionsEvent::Event {
                        event: TranscriptionsStreamEvent::Other(value),
                    };
                }
                None => {
                    for event in normalized {
                        yield StreamingTranscriptionsEvent::Event { event };
                    }
                }
            }

            yield StreamingTranscriptionsEvent::Done { stats };
        }));
    }

    let mut stream = sse::events(response);
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                break;
            }

            let Some(event) = frames.push::<TranscriptionsStreamEvent>(data)? else {
                continue;
            };

            let event = match normalizer.as_mut() {
                Some(normalizer) => normalizer.normalize(event),
                None => Some(event),
            };

            let Some(event) = event else {
                continue;
            };

            if let TranscriptionsStreamEvent::Error { error, .. } = &event {
                stats.error = Some(error.to_string());
                yield StreamingTranscriptionsEvent::EventError { event, stats };
                return;
            }

            record(&event, &mut stats, &mut pacer).await;

            let is_done = matches!(event, TranscriptionsStreamEvent::Done { .. });

            yield StreamingTranscriptionsEvent::Event { event };

            if is_done {
                break;
            }
        }

        if let Some(event) = normalizer.and_then(EventNormalizer::finish) {
            record(&event, &mut stats, &mut pacer).await;
            yield StreamingTranscriptionsEvent::Event { event };
        }

        yield StreamingTranscriptionsEvent::Done { stats };
    }))
}

async fn record(
    event: &TranscriptionsStreamEvent,
    stats: &mut StreamStats,
    pacer: &mut TpsThrottler<'_>,
) {
    match event {
        TranscriptionsStreamEvent::Delta { delta, .. } => {
            let tokens = delta.estimate_tokens();
            stats.output_tokens += tokens;
            stats.chunks.push(pacer.pace(tokens).await);
        }
        TranscriptionsStreamEvent::Done { usage, .. } => {
            if let Some(input_tokens) = usage.as_ref().and_then(|u| u.input_tokens) {
                stats.input_tokens_is_estimate = false;
                stats.input_tokens = input_tokens;
            }

            if let Some(output_tokens) = usage.as_ref().and_then(|u| u.output_tokens) {
                stats.output_tokens_is_estimate = false;
                stats.output_tokens = output_tokens;
            }
        }
        TranscriptionsStreamEvent::Error { .. } | TranscriptionsStreamEvent::Other(_) => {}
    }
}
