use crate::{
    chat_completion::{
        models::{
            api::{
                common::ChatCompletionUsage,
                request::non_streaming::NonStreamingChatCompletionRequestBody,
                response::non_streaming::{
                    NonStreamingChatCompletionChoice, NonStreamingChatCompletionResponse,
                    NonStreamingChatCompletionResponseBody,
                },
            },
            lib::{
                options::ChatCompletionOptions, streaming::response::StreamingChatCompletionEvent,
            },
        },
        streaming::streaming_chat_completion,
    },
    error::Error,
    stats::StreamStats,
    traits::estimate_tokens::EstimateTokens,
};

use itertools::Itertools;
use reqwest::IntoUrl;
use serde_json::Value;
use std::collections::HashMap;
use stefans_utils::prelude::MapInto;
use tokio_stream::StreamExt;

pub async fn non_streaming_chat_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions<'a>>>,
) -> Result<NonStreamingChatCompletionResponse, Error> {
    let body = body.into();
    let model = body.common.model.clone();
    let prompt_tokens_estimate = body.common.estimate_tokens();

    let mut stream = streaming_chat_completion(url, body, options).await?;

    #[derive(Default)]
    struct Acc {
        choices: HashMap<u32, NonStreamingChatCompletionChoice>,
        usage: Option<ChatCompletionUsage>,
        stats: Option<StreamStats>,
        error: Option<Value>,
    }

    let mut acc = Acc::default();

    while let Some(event) = stream.try_next().await? {
        let (event_stats_opt, chunk_opt) = match event {
            StreamingChatCompletionEvent::Done { stats } => (Some(stats), None),
            StreamingChatCompletionEvent::Chunk { chunk } => (None, Some(chunk)),
            StreamingChatCompletionEvent::ChunkError { stats, chunk } => (Some(stats), Some(chunk)),
        };

        if event_stats_opt.is_some() {
            acc.stats = event_stats_opt;
        };

        if let Some(chunk) = chunk_opt {
            if chunk.usage.is_some() {
                acc.usage = chunk.usage;
            };

            if chunk.error.is_some() {
                acc.error = chunk.error;
            }

            if let Some(choices) = chunk.choices {
                for choice in choices {
                    let index = choice.common.index;

                    let value = match acc.choices.remove(&index) {
                        Some(prev) => prev + choice,
                        None => choice.into(),
                    };

                    acc.choices.insert(index, value);
                }
            }
        }
    }

    let choices = acc.choices.into_values().collect_vec();

    let usage = acc
        .usage
        .or_else(|| acc.stats.as_ref().map_into())
        .unwrap_or_else(|| {
            ChatCompletionUsage::default()
                .with_prompt_tokens(prompt_tokens_estimate)
                .with_completion_tokens(choices.iter().map(|c| c.message.estimate_tokens()).sum())
        });

    Ok(NonStreamingChatCompletionResponse {
        body: NonStreamingChatCompletionResponseBody {
            object: Default::default(),
            model,
            choices,
            usage,
        },
        stats: acc.stats,
        error: acc.error,
    })
}
