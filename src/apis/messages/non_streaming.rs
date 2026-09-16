use crate::{
    apis::messages::{
        models::{
            api::{
                common::{ContentBlock, ContentBlockDelta},
                request::non_streaming::NonStreamingMessagesRequestBody,
                response::{
                    non_streaming::{MessagesResponseBody, NonStreamingMessagesResponse},
                    streaming::MessagesStreamEvent,
                },
            },
            lib::{options::MessagesOptions, streaming::response::StreamingMessagesEvent},
        },
        streaming::streaming_messages,
    },
    error::Error,
    stats::StreamStats,
    traits::merge::Merge,
};

use indexmap::IndexMap;
use reqwest::IntoUrl;
use serde_json::Value;
use tokio_stream::StreamExt;

pub async fn non_streaming_messages<'a>(
    url: impl IntoUrl,
    body: impl Into<NonStreamingMessagesRequestBody>,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Result<NonStreamingMessagesResponse, Error> {
    let mut stream = streaming_messages(url, body.into(), options).await?;

    #[derive(Default)]
    struct Acc {
        body: MessagesResponseBody,
        blocks: IndexMap<u32, (ContentBlock, String)>,
        stats: Option<StreamStats>,
        error: Option<Value>,
    }

    let mut acc = Acc::default();

    while let Some(item) = stream.try_next().await? {
        let event = match item {
            StreamingMessagesEvent::Done { stats } => {
                acc.stats = Some(stats);
                continue;
            }
            StreamingMessagesEvent::Event { event } => event,
            StreamingMessagesEvent::EventError { stats, event } => {
                acc.stats = Some(stats);
                event
            }
        };

        match event {
            MessagesStreamEvent::MessageStart { message, .. } => {
                acc.body = message;
                acc.body.content.clear();
            }
            MessagesStreamEvent::ContentBlockStart {
                index,
                content_block,
                ..
            } => {
                acc.blocks.insert(index, (content_block, String::new()));
            }
            MessagesStreamEvent::ContentBlockDelta { index, delta, .. } => {
                if let Some((block, partial_json)) = acc.blocks.get_mut(&index) {
                    apply_delta(block, partial_json, delta);
                }
            }
            MessagesStreamEvent::ContentBlockStop { index, .. } => {
                if let Some((ContentBlock::ToolUse { input, .. }, partial_json)) =
                    acc.blocks.get_mut(&index)
                    && !partial_json.is_empty()
                {
                    *input = serde_json::from_str(partial_json)
                        .unwrap_or_else(|_| Value::String(std::mem::take(partial_json)));
                }
            }
            MessagesStreamEvent::MessageDelta { delta, usage, .. } => {
                if delta.stop_reason.is_some() {
                    acc.body.stop_reason = delta.stop_reason;
                }

                if delta.stop_sequence.is_some() {
                    acc.body.stop_sequence = delta.stop_sequence;
                }

                acc.body
                    .additional_properties
                    .extend(delta.additional_properties);

                if let Some(usage) = usage {
                    acc.body.usage = std::mem::take(&mut acc.body.usage).merge(usage);
                }
            }
            MessagesStreamEvent::Error { error, .. } => {
                acc.error = Some(error);
            }
            _ => {}
        }
    }

    acc.blocks.sort_keys();
    acc.body.content = acc.blocks.into_values().map(|(block, _)| block).collect();

    if let Some(stats) = &acc.stats {
        acc.body
            .usage
            .input_tokens
            .get_or_insert(stats.input_tokens);
        acc.body
            .usage
            .output_tokens
            .get_or_insert(stats.output_tokens);
    }

    Ok(NonStreamingMessagesResponse {
        body: acc.body,
        stats: acc.stats,
        error: acc.error,
    })
}

fn apply_delta(block: &mut ContentBlock, partial_json: &mut String, delta: ContentBlockDelta) {
    match (block, delta) {
        (ContentBlock::Text { text, .. }, ContentBlockDelta::TextDelta { text: t, .. }) => {
            text.push_str(&t);
        }
        (
            ContentBlock::Thinking { thinking, .. },
            ContentBlockDelta::ThinkingDelta { thinking: t, .. },
        ) => {
            thinking.push_str(&t);
        }
        (
            ContentBlock::Thinking { signature, .. },
            ContentBlockDelta::SignatureDelta { signature: s, .. },
        ) => {
            signature.push_str(&s);
        }
        (
            ContentBlock::ToolUse { .. },
            ContentBlockDelta::InputJsonDelta {
                partial_json: p, ..
            },
        ) => {
            partial_json.push_str(&p);
        }
        _ => {}
    }
}
