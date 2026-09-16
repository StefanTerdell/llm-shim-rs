use crate::{
    apis::transcriptions::models::{
        api::{request::TranscriptionsRequest, response::streaming::TranscriptionsStreamEvent},
        lib::{
            options::TranscriptionsOptions,
            streaming::response::{StreamingTranscriptionsEvent, StreamingTranscriptionsResponse},
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

pub async fn streaming_transcriptions<'a>(
    url: impl IntoUrl,
    request: TranscriptionsRequest,
    options: impl Into<Option<TranscriptionsOptions<'a>>>,
) -> Result<StreamingTranscriptionsResponse<'a>, Error> {
    let options = options.into().unwrap_or_default();
    let client = options.client.unwrap_or_default();
    let tps_throttler = options.tps_throttler;

    let mut builder = client.post(url).multipart(request.into_form(Some(true))?);

    if let Some(bearer_token) = options.bearer_token {
        builder = builder.bearer_auth(bearer_token.expose());
    }

    let mut stats = StreamStats::new(Instant::now(), 0);
    let mut pacer = TpsThrottler::new(tps_throttler, stats.requested);
    let mut stream = sse::send(builder).await?;
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                break;
            }

            let Some(event) = frames.push::<TranscriptionsStreamEvent>(data)? else {
                continue;
            };

            match &event {
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
                TranscriptionsStreamEvent::Error { error, .. } => {
                    stats.error = Some(error.to_string());
                    yield StreamingTranscriptionsEvent::EventError { event, stats };
                    return;
                }
                TranscriptionsStreamEvent::Other(_) => {}
            }

            let is_done = matches!(event, TranscriptionsStreamEvent::Done { .. });

            yield StreamingTranscriptionsEvent::Event { event };

            if is_done {
                break;
            }
        }

        yield StreamingTranscriptionsEvent::Done { stats };
    }))
}
