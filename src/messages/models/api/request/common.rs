use indexmap::IndexMap;
use serde_json::Value;

use crate::messages::models::api::common::ContentBlock;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonMessagesRequestBody {
    pub messages: Vec<MessageParam>,
    pub system: Option<MessageContent>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct MessageParam {
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
