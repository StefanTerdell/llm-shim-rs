use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct ResponsesUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum OutputItem {
    #[serde(rename = "message")]
    Message {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        content: Vec<ContentPart>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "reasoning")]
    Reasoning {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        summary: Vec<ReasoningPart>,
        #[serde(default)]
        content: Option<Vec<ReasoningPart>>,
        #[serde(default)]
        status: Option<String>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        call_id: Option<String>,
        name: String,
        #[serde(default)]
        arguments: String,
        #[serde(default)]
        status: Option<String>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl OutputItem {
    pub fn message(id: Option<String>) -> Self {
        Self::Message {
            id,
            role: Some("assistant".to_string()),
            status: None,
            content: vec![],
            additional_properties: Default::default(),
        }
    }

    pub fn reasoning(id: Option<String>) -> Self {
        Self::Reasoning {
            id,
            summary: vec![],
            content: None,
            status: None,
            additional_properties: Default::default(),
        }
    }

    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Message { id, .. }
            | Self::Reasoning { id, .. }
            | Self::FunctionCall { id, .. } => id.as_deref(),
            Self::Other(value) => value.get("id").and_then(Value::as_str),
        }
    }

    pub fn status_mut(&mut self) -> Option<&mut Option<String>> {
        match self {
            Self::Message { status, .. }
            | Self::Reasoning { status, .. }
            | Self::FunctionCall { status, .. } => Some(status),
            Self::Other(_) => None,
        }
    }
}

#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "output_text")]
    OutputText {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "refusal")]
    Refusal {
        refusal: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "reasoning_text")]
    ReasoningText {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl ContentPart {
    pub fn output_text(text: impl Into<String>) -> Self {
        Self::OutputText {
            text: text.into(),
            additional_properties: IndexMap::from([(
                "annotations".to_string(),
                Value::Array(vec![]),
            )]),
        }
    }

    pub fn reasoning_text(text: impl Into<String>) -> Self {
        Self::ReasoningText {
            text: text.into(),
            additional_properties: Default::default(),
        }
    }
}

#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum ReasoningPart {
    #[serde(rename = "summary_text")]
    SummaryText {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "reasoning_text")]
    ReasoningText {
        text: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl ReasoningPart {
    pub fn summary_text(text: impl Into<String>) -> Self {
        Self::SummaryText {
            text: text.into(),
            additional_properties: Default::default(),
        }
    }

    pub fn reasoning_text(text: impl Into<String>) -> Self {
        Self::ReasoningText {
            text: text.into(),
            additional_properties: Default::default(),
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::SummaryText { text, .. } | Self::ReasoningText { text, .. } => Some(text),
            Self::Other(_) => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::SummaryText { text, .. } | Self::ReasoningText { text, .. } => Some(text),
            Self::Other(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_output_items_and_keeps_unknown_ones() {
        let item: OutputItem = serde_json::from_value(json!({"type": "message", "id": "msg_1", "status": "completed", "role": "assistant", "content": [{"type": "output_text", "text": "hi", "annotations": []}]})).unwrap();
        let OutputItem::Message { content, .. } = &item else {
            panic!()
        };
        assert_eq!(content[0], ContentPart::output_text("hi"));

        let item: OutputItem = serde_json::from_value(json!({"type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "s"}], "content": [{"type": "reasoning_text", "text": "r"}]})).unwrap();
        let OutputItem::Reasoning {
            summary, content, ..
        } = &item
        else {
            panic!()
        };
        assert_eq!(summary[0], ReasoningPart::summary_text("s"));
        assert_eq!(
            content.as_ref().unwrap()[0],
            ReasoningPart::reasoning_text("r")
        );

        let item: OutputItem = serde_json::from_value(
            json!({"type": "web_search_call", "id": "ws_1", "status": "completed"}),
        )
        .unwrap();
        assert!(matches!(item, OutputItem::Other(_)));
        assert_eq!(item.id(), Some("ws_1"));
    }

    #[test]
    fn round_trips_a_function_call_item() {
        let v = json!({"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "get_weather", "arguments": "{\"city\":\"Paris\"}", "status": "completed"});
        let item: OutputItem = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(&item).unwrap(), v);
    }
}
