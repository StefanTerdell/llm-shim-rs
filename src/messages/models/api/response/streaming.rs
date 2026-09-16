use indexmap::IndexMap;
use serde_json::Value;

use crate::messages::models::api::{
    common::{ContentBlock, ContentBlockDelta, MessagesUsage},
    response::non_streaming::MessagesResponseBody,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum MessagesStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: MessagesResponseBody,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlock,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u32,
        delta: ContentBlockDelta,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: u32,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDelta,
        usage: Option<MessagesUsage>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "message_stop")]
    MessageStop {
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "error")]
    Error {
        error: Value,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl MessagesStreamEvent {
    pub fn content_block_start(index: u32, content_block: ContentBlock) -> Self {
        Self::ContentBlockStart {
            index,
            content_block,
            additional_properties: Default::default(),
        }
    }

    pub fn content_block_delta(index: u32, delta: ContentBlockDelta) -> Self {
        Self::ContentBlockDelta {
            index,
            delta,
            additional_properties: Default::default(),
        }
    }

    pub fn content_block_stop(index: u32) -> Self {
        Self::ContentBlockStop {
            index,
            additional_properties: Default::default(),
        }
    }

    pub fn block_index(&self) -> Option<u32> {
        match self {
            Self::ContentBlockStart { index, .. }
            | Self::ContentBlockDelta { index, .. }
            | Self::ContentBlockStop { index, .. } => Some(*index),
            _ => None,
        }
    }

    pub fn set_block_index(&mut self, new_index: u32) {
        match self {
            Self::ContentBlockStart { index, .. }
            | Self::ContentBlockDelta { index, .. }
            | Self::ContentBlockStop { index, .. } => *index = new_index,
            _ => {}
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct MessageDelta {
    #[serde(default)]
    #[serialize_always]
    pub stop_reason: Option<String>,
    #[serde(default)]
    #[serialize_always]
    pub stop_sequence: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> MessagesStreamEvent {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn parses_documented_event_sequence() {
        let start = parse(
            json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "model": "claude-opus-5", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}),
        );
        let MessagesStreamEvent::MessageStart { message, .. } = start else {
            panic!()
        };
        assert_eq!(message.usage.input_tokens, Some(25));
        assert_eq!(message.additional_properties["type"], json!("message"));

        assert!(matches!(
            parse(json!({"type": "ping"})),
            MessagesStreamEvent::Other(_)
        ));

        let delta = parse(
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "Let me"}}),
        );
        assert_eq!(
            delta,
            MessagesStreamEvent::content_block_delta(0, ContentBlockDelta::thinking("Let me"))
        );

        let md = parse(
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 15}}),
        );
        let MessagesStreamEvent::MessageDelta { delta, usage, .. } = md else {
            panic!()
        };
        assert_eq!(delta.stop_reason.as_deref(), Some("end_turn"));
        assert_eq!(usage.unwrap().output_tokens, Some(15));

        let err = parse(
            json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}),
        );
        assert!(matches!(err, MessagesStreamEvent::Error { .. }));

        let unknown = parse(json!({"type": "some_future_event", "payload": 1}));
        assert!(matches!(unknown, MessagesStreamEvent::Other(_)));
    }

    #[test]
    fn round_trips_to_the_same_json() {
        let v = json!({"type": "content_block_start", "index": 1, "content_block": {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {}}});
        assert_eq!(serde_json::to_value(parse(v.clone())).unwrap(), v);
    }
}
