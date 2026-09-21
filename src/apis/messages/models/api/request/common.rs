use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

use crate::apis::messages::models::api::common::ContentBlock;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonMessagesRequestBody {
    pub model: Option<String>,
    pub messages: Vec<MessageParam>,
    pub system: Option<MessageContent>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl CommonMessagesRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
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
