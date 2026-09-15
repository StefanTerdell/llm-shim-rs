use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    messages::models::api::common::{ContentBlock, MessagesUsage},
    stats::StreamStats,
};

pub struct NonStreamingMessagesResponse {
    pub body: MessagesResponseBody,
    pub stats: Option<StreamStats>,
    pub error: Option<Value>,
}

#[derive(..ApiModel, Default)]
pub struct MessagesResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub usage: MessagesUsage,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
