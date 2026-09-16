use indexmap::IndexMap;
use serde_json::Value;

use crate::apis::transcriptions::models::api::response::non_streaming::TranscriptionsUsage;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum TranscriptionsStreamEvent {
    #[serde(rename = "transcript.text.delta")]
    Delta {
        delta: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "transcript.text.done")]
    Done {
        text: String,
        usage: Option<TranscriptionsUsage>,
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
