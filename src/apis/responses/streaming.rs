use crate::{
    apis::responses::models::{
        api::{
            request::streaming::StreamingResponsesRequestBody,
            response::streaming::ResponsesStreamEvent,
        },
        lib::{
            options::{ResponsesOptions, reasoning_remapping::ResponsesReasoningRemappingState},
            streaming::response::{StreamingResponsesEvent, StreamingResponsesResponse},
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

pub async fn streaming_responses<'a>(
    url: impl IntoUrl,
    body: impl Into<StreamingResponsesRequestBody>,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Result<StreamingResponsesResponse<'a>, Error> {
    let body = body.into();
    let options = options.into().unwrap_or_default();

    let client = options.client.unwrap_or_default();
    let tps_throttler = options.tps_throttler;
    let mut remapper = options
        .reasoning_remapping
        .map(ResponsesReasoningRemappingState::new);

    let mut request = client.post(url).json(&body);

    if let Some(bearer_token) = options.bearer_token {
        request = request.bearer_auth(bearer_token.expose());
    }

    let mut stats = StreamStats::new(Instant::now(), body.common.estimate_tokens());
    let mut pacer = TpsThrottler::new(tps_throttler, stats.requested);
    let mut stream = sse::send(request).await?;
    let mut frames = JsonFrameBuffer::default();

    Ok(Box::pin(try_stream! {
        while let Some(Event { data, .. }) = stream.next().await.transpose()? {
            if data == "[DONE]" {
                break;
            }

            let Some(event) = frames.push::<ResponsesStreamEvent>(data)? else {
                continue;
            };

            let mut failed = false;

            match &event {
                ResponsesStreamEvent::Created { response, .. }
                | ResponsesStreamEvent::InProgress { response, .. }
                | ResponsesStreamEvent::Completed { response, .. }
                | ResponsesStreamEvent::Incomplete { response, .. }
                | ResponsesStreamEvent::Failed { response, .. } => {
                    if let Some(Some(usage)) = &response.usage {
                        if let Some(input_tokens) = usage.input_tokens {
                            stats.input_tokens_is_estimate = false;
                            stats.input_tokens = input_tokens;
                        }

                        if let Some(output_tokens) = usage.output_tokens {
                            stats.output_tokens_is_estimate = false;
                            stats.output_tokens = output_tokens;
                        }
                    }

                    if matches!(event, ResponsesStreamEvent::Failed { .. }) {
                        failed = true;
                        stats.error = Some(
                            response
                                .error
                                .as_ref()
                                .and_then(|e| e.as_ref())
                                .map(|e| e.to_string())
                                .unwrap_or_else(|| "response.failed".to_string()),
                        );
                    }
                }
                ResponsesStreamEvent::OutputItemAdded { item, .. } => {
                    let tokens = item.estimate_tokens();
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                ResponsesStreamEvent::ContentPartAdded { part, .. } => {
                    let tokens = part.estimate_tokens();
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                ResponsesStreamEvent::ReasoningSummaryPartAdded { part, .. } => {
                    let tokens = part.text().map(EstimateTokens::estimate_tokens).unwrap_or(0);
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                ResponsesStreamEvent::OutputTextDelta { delta, .. }
                | ResponsesStreamEvent::RefusalDelta { delta, .. }
                | ResponsesStreamEvent::ReasoningSummaryTextDelta { delta, .. }
                | ResponsesStreamEvent::ReasoningTextDelta { delta, .. }
                | ResponsesStreamEvent::FunctionCallArgumentsDelta { delta, .. } => {
                    let tokens = delta.estimate_tokens();
                    stats.output_tokens += tokens;
                    stats.chunks.push(pacer.pace(tokens).await);
                }
                ResponsesStreamEvent::Error { code, message, param, .. } => {
                    stats.error = Some(
                        serde_json::json!({"code": code, "message": message, "param": param})
                            .to_string(),
                    );
                    failed = true;
                }
                _ => {}
            }

            if failed {
                let event = match &mut remapper {
                    Some(remapper) => {
                        let mut events = remapper.apply(event);
                        events.extend(remapper.flush());
                        let last = events.pop();

                        for event in events {
                            yield StreamingResponsesEvent::Event { event };
                        }

                        match last {
                            Some(event) => event,
                            None => return,
                        }
                    }
                    None => event,
                };

                yield StreamingResponsesEvent::EventError { event, stats };
                return;
            }

            let is_terminal = event.is_terminal();

            match &mut remapper {
                Some(remapper) => {
                    for event in remapper.apply(event) {
                        yield StreamingResponsesEvent::Event { event };
                    }
                }
                None => yield StreamingResponsesEvent::Event { event },
            }

            if is_terminal {
                break;
            }
        }

        if let Some(remapper) = &mut remapper {
            for event in remapper.flush() {
                yield StreamingResponsesEvent::Event { event };
            }
        }

        yield StreamingResponsesEvent::Done { stats };
    }))
}
