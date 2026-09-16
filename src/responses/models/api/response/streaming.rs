use indexmap::IndexMap;
use serde_json::Value;

use crate::responses::models::api::{
    common::{ContentPart, OutputItem, ReasoningPart},
    response::non_streaming::ResponseBody,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum ResponsesStreamEvent {
    #[serde(rename = "response.created")]
    Created {
        response: ResponseBody,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.in_progress")]
    InProgress {
        response: ResponseBody,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.completed")]
    Completed {
        response: ResponseBody,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.incomplete")]
    Incomplete {
        response: ResponseBody,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.failed")]
    Failed {
        response: ResponseBody,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: OutputItem,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: u32,
        item: OutputItem,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        part: ContentPart,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        part: ContentPart,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        delta: String,
        #[serde(default)]
        logprobs: Option<Vec<Value>>,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        text: String,
        #[serde(default)]
        logprobs: Option<Vec<Value>>,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.refusal.delta")]
    RefusalDelta {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.refusal.done")]
    RefusalDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        refusal: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        summary_index: u32,
        part: ReasoningPart,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        summary_index: u32,
        part: ReasoningPart,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        summary_index: u32,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        summary_index: u32,
        text: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_text.delta")]
    ReasoningTextDelta {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.reasoning_text.done")]
    ReasoningTextDone {
        #[serde(default)]
        item_id: Option<String>,
        output_index: u32,
        #[serde(default)]
        content_index: u32,
        text: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        output_index: u32,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        output_index: u32,
        arguments: String,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "error")]
    Error {
        #[serde(default)]
        #[serialize_always]
        code: Option<Value>,
        #[serde(default)]
        #[serialize_always]
        message: Option<String>,
        #[serde(default)]
        #[serialize_always]
        param: Option<Value>,
        #[serde(default)]
        sequence_number: Option<u64>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

impl ResponsesStreamEvent {
    pub fn output_index(&self) -> Option<u32> {
        match self {
            Self::OutputItemAdded { output_index, .. }
            | Self::OutputItemDone { output_index, .. }
            | Self::ContentPartAdded { output_index, .. }
            | Self::ContentPartDone { output_index, .. }
            | Self::OutputTextDelta { output_index, .. }
            | Self::OutputTextDone { output_index, .. }
            | Self::RefusalDelta { output_index, .. }
            | Self::RefusalDone { output_index, .. }
            | Self::ReasoningSummaryPartAdded { output_index, .. }
            | Self::ReasoningSummaryPartDone { output_index, .. }
            | Self::ReasoningSummaryTextDelta { output_index, .. }
            | Self::ReasoningSummaryTextDone { output_index, .. }
            | Self::ReasoningTextDelta { output_index, .. }
            | Self::ReasoningTextDone { output_index, .. }
            | Self::FunctionCallArgumentsDelta { output_index, .. }
            | Self::FunctionCallArgumentsDone { output_index, .. } => Some(*output_index),
            Self::Other(value) => value
                .get("output_index")
                .and_then(Value::as_u64)
                .map(|i| i as u32),
            _ => None,
        }
    }

    pub fn set_output_index(&mut self, new_index: u32) {
        match self {
            Self::OutputItemAdded { output_index, .. }
            | Self::OutputItemDone { output_index, .. }
            | Self::ContentPartAdded { output_index, .. }
            | Self::ContentPartDone { output_index, .. }
            | Self::OutputTextDelta { output_index, .. }
            | Self::OutputTextDone { output_index, .. }
            | Self::RefusalDelta { output_index, .. }
            | Self::RefusalDone { output_index, .. }
            | Self::ReasoningSummaryPartAdded { output_index, .. }
            | Self::ReasoningSummaryPartDone { output_index, .. }
            | Self::ReasoningSummaryTextDelta { output_index, .. }
            | Self::ReasoningSummaryTextDone { output_index, .. }
            | Self::ReasoningTextDelta { output_index, .. }
            | Self::ReasoningTextDone { output_index, .. }
            | Self::FunctionCallArgumentsDelta { output_index, .. }
            | Self::FunctionCallArgumentsDone { output_index, .. } => *output_index = new_index,
            Self::Other(value) => {
                if let Some(object) = value.as_object_mut()
                    && object.contains_key("output_index")
                {
                    object.insert("output_index".to_string(), Value::from(new_index));
                }
            }
            _ => {}
        }
    }

    pub fn sequence_number_mut(&mut self) -> Option<&mut Option<u64>> {
        match self {
            Self::Created {
                sequence_number, ..
            }
            | Self::InProgress {
                sequence_number, ..
            }
            | Self::Completed {
                sequence_number, ..
            }
            | Self::Incomplete {
                sequence_number, ..
            }
            | Self::Failed {
                sequence_number, ..
            }
            | Self::OutputItemAdded {
                sequence_number, ..
            }
            | Self::OutputItemDone {
                sequence_number, ..
            }
            | Self::ContentPartAdded {
                sequence_number, ..
            }
            | Self::ContentPartDone {
                sequence_number, ..
            }
            | Self::OutputTextDelta {
                sequence_number, ..
            }
            | Self::OutputTextDone {
                sequence_number, ..
            }
            | Self::RefusalDelta {
                sequence_number, ..
            }
            | Self::RefusalDone {
                sequence_number, ..
            }
            | Self::ReasoningSummaryPartAdded {
                sequence_number, ..
            }
            | Self::ReasoningSummaryPartDone {
                sequence_number, ..
            }
            | Self::ReasoningSummaryTextDelta {
                sequence_number, ..
            }
            | Self::ReasoningSummaryTextDone {
                sequence_number, ..
            }
            | Self::ReasoningTextDelta {
                sequence_number, ..
            }
            | Self::ReasoningTextDone {
                sequence_number, ..
            }
            | Self::FunctionCallArgumentsDelta {
                sequence_number, ..
            }
            | Self::FunctionCallArgumentsDone {
                sequence_number, ..
            }
            | Self::Error {
                sequence_number, ..
            } => Some(sequence_number),
            Self::Other(_) => None,
        }
    }

    pub fn sequence_number(&self) -> Option<u64> {
        match self {
            Self::Other(value) => value.get("sequence_number").and_then(Value::as_u64),
            _ => {
                let mut copy = self.clone();
                copy.sequence_number_mut().copied().flatten()
            }
        }
    }

    pub fn set_sequence_number(&mut self, n: u64) {
        match self {
            Self::Other(value) => {
                if let Some(object) = value.as_object_mut()
                    && object.contains_key("sequence_number")
                {
                    object.insert("sequence_number".to_string(), Value::from(n));
                }
            }
            _ => {
                if let Some(sequence_number) = self.sequence_number_mut() {
                    *sequence_number = Some(n);
                }
            }
        }
    }

    pub fn set_item_id(&mut self, new_id: &str) {
        match self {
            Self::ContentPartAdded { item_id, .. }
            | Self::ContentPartDone { item_id, .. }
            | Self::OutputTextDelta { item_id, .. }
            | Self::OutputTextDone { item_id, .. }
            | Self::RefusalDelta { item_id, .. }
            | Self::RefusalDone { item_id, .. }
            | Self::ReasoningSummaryPartAdded { item_id, .. }
            | Self::ReasoningSummaryPartDone { item_id, .. }
            | Self::ReasoningSummaryTextDelta { item_id, .. }
            | Self::ReasoningSummaryTextDone { item_id, .. }
            | Self::ReasoningTextDelta { item_id, .. }
            | Self::ReasoningTextDone { item_id, .. } => *item_id = Some(new_id.to_string()),
            _ => {}
        }
    }

    pub fn content_index(&self) -> Option<u32> {
        match self {
            Self::ContentPartAdded { content_index, .. }
            | Self::ContentPartDone { content_index, .. }
            | Self::OutputTextDelta { content_index, .. }
            | Self::OutputTextDone { content_index, .. }
            | Self::RefusalDelta { content_index, .. }
            | Self::RefusalDone { content_index, .. }
            | Self::ReasoningTextDelta { content_index, .. }
            | Self::ReasoningTextDone { content_index, .. } => Some(*content_index),
            _ => None,
        }
    }

    pub fn set_content_index(&mut self, new_index: u32) {
        match self {
            Self::ContentPartAdded { content_index, .. }
            | Self::ContentPartDone { content_index, .. }
            | Self::OutputTextDelta { content_index, .. }
            | Self::OutputTextDone { content_index, .. }
            | Self::RefusalDelta { content_index, .. }
            | Self::RefusalDone { content_index, .. }
            | Self::ReasoningTextDelta { content_index, .. }
            | Self::ReasoningTextDone { content_index, .. } => *content_index = new_index,
            _ => {}
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Incomplete { .. } | Self::Failed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> ResponsesStreamEvent {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn parses_documented_events() {
        let e = parse(
            json!({"type": "response.output_text.delta", "item_id": "msg_1", "output_index": 1, "content_index": 0, "delta": "Hi", "logprobs": [], "sequence_number": 7}),
        );
        let ResponsesStreamEvent::OutputTextDelta {
            delta,
            sequence_number,
            logprobs,
            ..
        } = &e
        else {
            panic!()
        };
        assert_eq!(delta, "Hi");
        assert_eq!(*sequence_number, Some(7));
        assert_eq!(logprobs.as_ref().unwrap().len(), 0);
        assert_eq!(e.output_index(), Some(1));

        let e = parse(
            json!({"type": "response.reasoning_summary_text.delta", "item_id": "rs_1", "output_index": 0, "summary_index": 0, "delta": "thinking", "sequence_number": 3}),
        );
        assert!(matches!(
            e,
            ResponsesStreamEvent::ReasoningSummaryTextDelta { .. }
        ));

        let e = parse(
            json!({"type": "response.reasoning_text.delta", "item_id": "rs_1", "output_index": 0, "content_index": 0, "delta": "raw", "sequence_number": 3}),
        );
        assert!(matches!(e, ResponsesStreamEvent::ReasoningTextDelta { .. }));

        let e = parse(
            json!({"type": "response.completed", "sequence_number": 20, "response": {"id": "resp_1", "object": "response", "status": "completed", "output": [], "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15, "output_tokens_details": {"reasoning_tokens": 2}}}}),
        );
        let ResponsesStreamEvent::Completed { response, .. } = &e else {
            panic!()
        };
        assert_eq!(
            response.usage.clone().flatten().unwrap().output_tokens,
            Some(5)
        );
        assert!(e.is_terminal());

        let e = parse(
            json!({"type": "error", "code": "server_error", "message": "boom", "param": null, "sequence_number": 4}),
        );
        assert!(matches!(e, ResponsesStreamEvent::Error { .. }));

        let e = parse(
            json!({"type": "response.web_search_call.searching", "output_index": 2, "item_id": "ws_1", "sequence_number": 5}),
        );
        assert!(matches!(e, ResponsesStreamEvent::Other(_)));
        assert_eq!(e.output_index(), Some(2));
    }

    #[test]
    fn events_without_sequence_numbers_or_item_ids_still_parse() {
        let e =
            parse(json!({"type": "response.output_text.delta", "output_index": 0, "delta": "x"}));
        let ResponsesStreamEvent::OutputTextDelta {
            item_id,
            content_index,
            sequence_number,
            ..
        } = &e
        else {
            panic!()
        };
        assert!(item_id.is_none());
        assert_eq!(*content_index, 0);
        assert!(sequence_number.is_none());
    }

    #[test]
    fn round_trips_output_item_added() {
        let v = json!({"type": "response.output_item.added", "output_index": 0, "item": {"type": "reasoning", "id": "rs_1", "summary": []}, "sequence_number": 1});
        assert_eq!(serde_json::to_value(parse(v.clone())).unwrap(), v);
    }
}
