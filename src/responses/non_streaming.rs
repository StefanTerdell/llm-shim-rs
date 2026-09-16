use crate::{
    error::Error,
    responses::{
        models::{
            api::{
                request::non_streaming::NonStreamingResponsesRequestBody,
                response::non_streaming::NonStreamingResponsesResponse,
            },
            lib::{
                assembler::ResponseAssembler, options::ResponsesOptions,
                streaming::response::StreamingResponsesEvent,
            },
        },
        streaming::streaming_responses,
    },
    stats::StreamStats,
};

use reqwest::IntoUrl;
use tokio_stream::StreamExt;

pub async fn non_streaming_responses<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingResponsesRequestBody>,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Result<NonStreamingResponsesResponse, Error> {
    let mut stream = streaming_responses(url, body.into(), options).await?;
    let mut assembler = ResponseAssembler::default();
    let mut stats: Option<StreamStats> = None;

    while let Some(item) = stream.try_next().await? {
        match item {
            StreamingResponsesEvent::Done { stats: s } => stats = Some(s),
            StreamingResponsesEvent::Event { event } => assembler.apply(&event),
            StreamingResponsesEvent::EventError { stats: s, event } => {
                assembler.apply(&event);
                stats = Some(s);
            }
        }
    }

    let error = assembler.error().cloned();
    let mut body = assembler.into_body();

    if let Some(stats) = &stats {
        let usage = body.usage.get_or_insert_default();
        usage.input_tokens.get_or_insert(stats.input_tokens);
        usage.output_tokens.get_or_insert(stats.output_tokens);
        usage
            .total_tokens
            .get_or_insert(usage.input_tokens.unwrap_or(0) + usage.output_tokens.unwrap_or(0));
    }

    Ok(NonStreamingResponsesResponse { body, stats, error })
}
