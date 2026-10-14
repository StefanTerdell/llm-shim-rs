use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::{
    apis::{
        chat_completion::models::api::response::streaming::StreamingChatCompletionChunk,
        messages::models::api::{
            common::{ContentBlock, ContentBlockDelta, MessagesUsage},
            response::{
                non_streaming::MessagesResponseBody,
                streaming::{MessageDelta, MessagesStreamEvent},
            },
        },
    },
    convert::messages_chat_completion::finish_reason_to_stop_reason,
    traits::merge::Merge,
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum OpenBlock {
    Thinking(u32),
    Text(u32),
    ToolUse(u32),
}

impl OpenBlock {
    fn index(self) -> u32 {
        match self {
            Self::Thinking(index) | Self::Text(index) | Self::ToolUse(index) => index,
        }
    }
}

/// Turns a chat completion stream into the event sequence an Anthropic client expects.
///
/// Stateful by necessity: Anthropic wraps each run of deltas in its own `content_block_start` and
/// `content_block_stop`, and numbers the blocks, none of which a chat completion chunk carries. Feed
/// every chunk to [`Self::chunk`] in order, then call [`Self::finish`] once the stream ends.
#[derive(Debug, Default)]
pub struct MessagesStreamConverter {
    started: bool,
    open: Option<OpenBlock>,
    next_index: u32,
    tool_blocks: IndexMap<u32, u32>,
    stop_reason: Option<String>,
    usage: MessagesUsage,
}

impl MessagesStreamConverter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chunk(&mut self, chunk: StreamingChatCompletionChunk) -> Vec<MessagesStreamEvent> {
        let StreamingChatCompletionChunk {
            error,
            choices,
            usage,
            additional_properties,
        } = chunk;

        let mut events = Vec::new();

        if let Some(error) = error {
            events.push(MessagesStreamEvent::Error {
                error,
                additional_properties: Default::default(),
            });
            return events;
        }

        if !self.started {
            self.started = true;
            events.push(self.message_start(&additional_properties));
        }

        if let Some(usage) = usage.as_ref() {
            self.usage = std::mem::take(&mut self.usage).merge(MessagesUsage::from(usage));
        }

        let Some(choice) = choices.into_iter().flatten().next() else {
            return events;
        };

        if let Some(finish_reason) = choice
            .common
            .additional_properties
            .get("finish_reason")
            .and_then(Value::as_str)
        {
            self.stop_reason = Some(finish_reason_to_stop_reason(finish_reason).to_string());
        }

        let delta = choice.delta;

        if let Some(thinking) = delta
            .common
            .reasoning_content()
            .or_else(|| delta.common.reasoning())
            .filter(|thinking| !thinking.is_empty())
        {
            let index = self.open_thinking(&mut events);
            events.push(MessagesStreamEvent::content_block_delta(
                index,
                ContentBlockDelta::thinking(thinking),
            ));
        }

        if let Some(text) = delta.content().filter(|text| !text.is_empty()) {
            let index = self.open_text(&mut events);
            events.push(MessagesStreamEvent::content_block_delta(
                index,
                ContentBlockDelta::text(text),
            ));
        }

        for call in delta.common.tool_calls.into_iter().flatten() {
            let call_index = call.index.unwrap_or(0);

            if let Some(name) = call.function.name.filter(|name| !name.is_empty()) {
                let mut block_properties = call.additional_properties.clone();
                block_properties.shift_remove("type");

                let index = self.open_tool_use(&mut events, call_index);

                events.push(MessagesStreamEvent::content_block_start(
                    index,
                    ContentBlock::ToolUse {
                        name,
                        input: json!({}),
                        additional_properties: block_properties,
                    },
                ));
            }

            if let Some(arguments) = call.function.arguments.filter(|a| !a.is_empty())
                && let Some(index) = self.tool_blocks.get(&call_index).copied()
            {
                events.push(MessagesStreamEvent::content_block_delta(
                    index,
                    ContentBlockDelta::InputJsonDelta {
                        partial_json: arguments,
                        additional_properties: Default::default(),
                    },
                ));
            }
        }

        events
    }

    pub fn finish(mut self) -> Vec<MessagesStreamEvent> {
        let mut events = Vec::new();

        if !self.started {
            events.push(self.message_start(&Default::default()));
        }

        self.close_open_block(&mut events);

        events.push(MessagesStreamEvent::MessageDelta {
            delta: MessageDelta {
                stop_reason: Some(self.stop_reason.unwrap_or_else(|| "end_turn".to_string())),
                stop_sequence: None,
                additional_properties: Default::default(),
            },
            usage: Some(self.usage),
            additional_properties: Default::default(),
        });

        events.push(MessagesStreamEvent::MessageStop {
            additional_properties: Default::default(),
        });

        events
    }

    fn message_start(&self, chunk_properties: &IndexMap<String, Value>) -> MessagesStreamEvent {
        let mut additional_properties =
            IndexMap::from([("type".to_string(), json!("message")), ("role".to_string(), json!("assistant"))]);

        for key in ["id", "model"] {
            if let Some(value) = chunk_properties.get(key) {
                additional_properties.insert(key.to_string(), value.clone());
            }
        }

        MessagesStreamEvent::MessageStart {
            message: MessagesResponseBody {
                content: vec![],
                stop_reason: None,
                stop_sequence: None,
                usage: self.usage.clone(),
                additional_properties,
            },
            additional_properties: Default::default(),
        }
    }

    fn close_open_block(&mut self, events: &mut Vec<MessagesStreamEvent>) {
        if let Some(open) = self.open.take() {
            events.push(MessagesStreamEvent::content_block_stop(open.index()));
        }
    }

    fn open_thinking(&mut self, events: &mut Vec<MessagesStreamEvent>) -> u32 {
        if let Some(OpenBlock::Thinking(index)) = self.open {
            return index;
        }

        self.close_open_block(events);

        let index = self.take_index();
        self.open = Some(OpenBlock::Thinking(index));
        events.push(MessagesStreamEvent::content_block_start(
            index,
            ContentBlock::thinking("", ""),
        ));

        index
    }

    fn open_text(&mut self, events: &mut Vec<MessagesStreamEvent>) -> u32 {
        if let Some(OpenBlock::Text(index)) = self.open {
            return index;
        }

        self.close_open_block(events);

        let index = self.take_index();
        self.open = Some(OpenBlock::Text(index));
        events.push(MessagesStreamEvent::content_block_start(
            index,
            ContentBlock::text(""),
        ));

        index
    }

    fn open_tool_use(&mut self, events: &mut Vec<MessagesStreamEvent>, call_index: u32) -> u32 {
        self.close_open_block(events);

        let index = self.take_index();
        self.open = Some(OpenBlock::ToolUse(index));
        self.tool_blocks.insert(call_index, index);

        index
    }

    fn take_index(&mut self) -> u32 {
        let index = self.next_index;
        self.next_index += 1;
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chunks: Vec<Value>) -> Vec<Value> {
        let mut converter = MessagesStreamConverter::new();
        let mut events = Vec::new();

        for chunk in chunks {
            events.extend(converter.chunk(serde_json::from_value(chunk).unwrap()));
        }

        events
            .into_iter()
            .chain(converter.finish())
            .map(|event| serde_json::to_value(event).unwrap())
            .collect()
    }

    fn text_chunk(text: &str) -> Value {
        json!({"choices": [{"index": 0, "delta": {"content": text}}]})
    }

    fn types(events: &[Value]) -> Vec<&str> {
        events
            .iter()
            .map(|event| event["type"].as_str().unwrap())
            .collect()
    }

    #[test]
    fn a_text_stream_is_wrapped_in_one_content_block() {
        let events = run(vec![
            text_chunk("Hel"),
            text_chunk("lo"),
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[2]["delta"], json!({"type": "text_delta", "text": "Hel"}));
        assert_eq!(events[1]["index"], json!(0));
        assert_eq!(events[4]["index"], json!(0));
        assert_eq!(events[5]["delta"]["stop_reason"], json!("end_turn"));
    }

    #[test]
    fn thinking_and_text_become_separate_numbered_blocks() {
        let events = run(vec![
            json!({"choices": [{"index": 0, "delta": {"reasoning_content": "hmm"}}]}),
            text_chunk("answer"),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[1]["content_block"]["type"], json!("thinking"));
        assert_eq!(events[1]["index"], json!(0));
        assert_eq!(events[4]["content_block"]["type"], json!("text"));
        assert_eq!(
            events[4]["index"],
            json!(1),
            "a second block must not reuse the first block's number"
        );
    }

    #[test]
    fn a_streamed_tool_call_opens_a_block_and_streams_its_arguments() {
        let events = run(vec![
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_1", "type": "function", "function": {"name": "lookup", "arguments": ""}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "{\"q\":"}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "\"x\"}"}}]}}]}),
            json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[1]["content_block"]["type"], json!("tool_use"));
        assert_eq!(events[1]["content_block"]["name"], json!("lookup"));
        assert_eq!(
            events[1]["content_block"]["id"],
            json!("call_1"),
            "the call id has to reach the client or it cannot answer the tool call"
        );
        assert_eq!(
            events[2]["delta"],
            json!({"type": "input_json_delta", "partial_json": "{\"q\":"}),
            "anthropic streams tool arguments as json fragments, same as openai does"
        );
        assert_eq!(events[5]["delta"]["stop_reason"], json!("tool_use"));
    }

    #[test]
    fn text_then_a_tool_call_closes_the_text_block_first() {
        let events = run(vec![
            text_chunk("let me look"),
            json!({"choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, "id": "call_1", "function": {"name": "lookup"}}]}}]}),
        ]);

        assert_eq!(
            types(&events),
            vec![
                "message_start",
                "content_block_start",
                "content_block_delta",
                "content_block_stop",
                "content_block_start",
                "content_block_stop",
                "message_delta",
                "message_stop",
            ]
        );
        assert_eq!(events[4]["index"], json!(1));
    }

    #[test]
    fn usage_is_carried_to_the_message_delta() {
        let events = run(vec![
            text_chunk("hi"),
            json!({"choices": [], "usage": {"prompt_tokens": 11, "completion_tokens": 4, "total_tokens": 15}}),
        ]);

        let message_delta = events.iter().find(|e| e["type"] == json!("message_delta")).unwrap();
        assert_eq!(
            message_delta["usage"],
            json!({"input_tokens": 11, "output_tokens": 4})
        );
    }

    #[test]
    fn the_message_start_carries_the_id_and_model() {
        let events = run(vec![
            json!({"id": "chatcmpl-1", "model": "m", "choices": [{"index": 0, "delta": {"content": "hi"}}]}),
        ]);

        assert_eq!(events[0]["message"]["id"], json!("chatcmpl-1"));
        assert_eq!(events[0]["message"]["model"], json!("m"));
        assert_eq!(events[0]["message"]["role"], json!("assistant"));
        assert_eq!(events[0]["message"]["content"], json!([]));
    }

    #[test]
    fn an_error_chunk_becomes_an_error_event() {
        let mut converter = MessagesStreamConverter::new();
        let events = converter.chunk(
            serde_json::from_value(json!({"error": {"message": "boom"}})).unwrap(),
        );

        assert_eq!(
            serde_json::to_value(&events[0]).unwrap(),
            json!({"type": "error", "error": {"message": "boom"}})
        );
    }

    #[test]
    fn a_stream_that_sent_nothing_still_closes_properly() {
        let events = run(vec![]);

        assert_eq!(types(&events), vec!["message_start", "message_delta", "message_stop"]);
    }
}
