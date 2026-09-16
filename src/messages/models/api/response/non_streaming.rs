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

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct MessagesResponseBody {
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    #[serde(default)]
    #[serialize_always]
    pub stop_reason: Option<String>,
    #[serde(default)]
    #[serialize_always]
    pub stop_sequence: Option<String>,
    #[serde(default)]
    pub usage: MessagesUsage,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
