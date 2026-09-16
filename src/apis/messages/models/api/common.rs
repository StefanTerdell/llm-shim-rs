use indexmap::IndexMap;
use serde_json::Value;

use crate::traits::merge::Merge;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct MessagesUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Merge for MessagesUsage {
    fn merge(mut self, rhs: Self) -> Self {
        self.input_tokens = rhs.input_tokens.or(self.input_tokens);
        self.output_tokens = rhs.output_tokens.or(self.output_tokens);
        self.additional_properties.extend(rhs.additional_properties);
        self
    }
}

#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        input: Value,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            additional_properties: Default::default(),
        }
    }

    pub fn thinking(thinking: impl Into<String>, signature: impl Into<String>) -> Self {
        Self::Thinking {
            thinking: thinking.into(),
            signature: signature.into(),
            additional_properties: Default::default(),
        }
    }
}

#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum ContentBlockDelta {
    #[serde(rename = "text_delta")]
    TextDelta {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta {
        thinking: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "signature_delta")]
    SignatureDelta {
        signature: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta {
        partial_json: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl ContentBlockDelta {
    pub fn text(text: impl Into<String>) -> Self {
        Self::TextDelta {
            text: text.into(),
            additional_properties: Default::default(),
        }
    }

    pub fn thinking(thinking: impl Into<String>) -> Self {
        Self::ThinkingDelta {
            thinking: thinking.into(),
            additional_properties: Default::default(),
        }
    }

    pub fn signature(signature: impl Into<String>) -> Self {
        Self::SignatureDelta {
            signature: signature.into(),
            additional_properties: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_known_block_types_and_keeps_unknown_ones() {
        let text: ContentBlock =
            serde_json::from_value(json!({"type": "text", "text": "hi", "citations": null}))
                .unwrap();
        let ContentBlock::Text {
            text,
            additional_properties,
        } = text
        else {
            panic!()
        };
        assert_eq!(text, "hi");
        assert!(additional_properties.contains_key("citations"));

        let thinking: ContentBlock =
            serde_json::from_value(json!({"type": "thinking", "thinking": "hmm"})).unwrap();
        assert_eq!(thinking, ContentBlock::thinking("hmm", ""));

        let other: ContentBlock =
            serde_json::from_value(json!({"type": "web_search_tool_result", "content": []}))
                .unwrap();
        assert!(matches!(other, ContentBlock::Other(_)));
    }

    #[test]
    fn serializes_tagged_blocks_back_to_the_wire_shape() {
        let json = serde_json::to_value(ContentBlock::thinking("a", "sig")).unwrap();
        assert_eq!(
            json,
            json!({"type": "thinking", "thinking": "a", "signature": "sig"})
        );

        let json = serde_json::to_value(ContentBlockDelta::text("x")).unwrap();
        assert_eq!(json, json!({"type": "text_delta", "text": "x"}));
    }

    #[test]
    fn parses_delta_types() {
        let d: ContentBlockDelta =
            serde_json::from_value(json!({"type": "input_json_delta", "partial_json": "{\"a\""}))
                .unwrap();
        assert!(matches!(d, ContentBlockDelta::InputJsonDelta { .. }));
        let d: ContentBlockDelta =
            serde_json::from_value(json!({"type": "citations_delta", "citation": {}})).unwrap();
        assert!(matches!(d, ContentBlockDelta::Other(_)));
    }
}
