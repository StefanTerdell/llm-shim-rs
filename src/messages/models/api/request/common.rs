use indexmap::IndexMap;
use serde_json::Value;

use crate::messages::models::api::common::ContentBlock;

#[derive(..ApiModel)]
pub struct CommonMessagesRequestBody {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<MessageParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<MessageContent>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct MessageParam {
    pub role: String,
    pub content: MessageContent,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}
