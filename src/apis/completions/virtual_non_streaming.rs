use crate::{
    apis::completions::{
        models::{
            api::{
                common::{CompletionChoice, CompletionUsage},
                request::NonStreamingCompletionRequestBody,
                response::{NonStreamingCompletionResponse, NonStreamingCompletionResponseBody},
            },
            lib::{options::CompletionsOptions, streaming::response::StreamingCompletionEvent},
        },
        streaming::streaming_completion,
    },
    error::Error,
    stats::StreamStats,
    traits::estimate_tokens::EstimateTokens,
};

use indexmap::IndexMap;
use reqwest::IntoUrl;
use serde_json::Value;
use std::collections::BTreeMap;
use tokio_stream::StreamExt;

pub async fn virtual_non_streaming_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingCompletionRequestBody>,
    options: impl Into<Option<CompletionsOptions<'a>>>,
) -> Result<NonStreamingCompletionResponse, Error> {
    let body = body.into();
    let model = body.common.model.clone();
    let prompt_tokens_estimate = body.common.estimate_tokens();

    let mut stream = streaming_completion(url, body, options).await?;

    let mut choices: BTreeMap<u32, CompletionChoice> = BTreeMap::new();
    let mut usage: Option<CompletionUsage> = None;
    let mut stats: Option<StreamStats> = None;
    let mut error: Option<Value> = None;
    let mut metadata: IndexMap<String, Value> = IndexMap::new();

    while let Some(event) = stream.try_next().await? {
        let chunk = match event {
            StreamingCompletionEvent::Done { stats: done } => {
                stats = Some(done);
                None
            }
            StreamingCompletionEvent::Chunk { chunk } => Some(chunk),
            StreamingCompletionEvent::ChunkError {
                stats: failed,
                chunk,
            } => {
                stats = Some(failed);
                Some(chunk)
            }
        };

        let Some(mut chunk) = chunk else {
            continue;
        };

        chunk.additional_properties.shift_remove("object");
        chunk.additional_properties.shift_remove("model");
        metadata.extend(chunk.additional_properties);

        if chunk.usage.is_some() {
            usage = chunk.usage;
        }

        if chunk.error.is_some() {
            error = chunk.error;
        }

        for choice in chunk.choices.into_iter().flatten() {
            let index = choice.index;
            let merged = match choices.remove(&index) {
                Some(previous) => previous.append(choice),
                None => choice,
            };
            choices.insert(index, merged);
        }
    }

    let choices: Vec<CompletionChoice> = choices.into_values().collect();

    let usage = usage
        .or_else(|| stats.as_ref().map(CompletionUsage::from))
        .unwrap_or_else(|| {
            let mut fallback = StreamStats::new(std::time::Instant::now(), prompt_tokens_estimate);
            fallback.output_tokens = choices.iter().map(|c| c.text.estimate_tokens()).sum();
            CompletionUsage::from(&fallback)
        });

    Ok(NonStreamingCompletionResponse {
        body: NonStreamingCompletionResponseBody {
            object: Default::default(),
            model,
            choices,
            usage: Some(usage),
            error: None,
            additional_properties: metadata,
        },
        stats,
        error,
    })
}
