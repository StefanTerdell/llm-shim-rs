use indexmap::IndexMap;
use serde_json::Value;

use crate::chat_completion::models::api::{
    common::ChatCompletionUsage,
    response::common::{ChatCompletionResponseMessage, CommonChatCompletionChoice},
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingChatCompletionChunk {
    pub error: Option<Value>,
    pub choices: Option<Vec<StreamingChatCompletionChoice>>,
    pub usage: Option<ChatCompletionUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct StreamingChatCompletionChoice {
    pub delta: ChatCompletionResponseMessage,
    #[serde(flatten)]
    pub common: CommonChatCompletionChoice,
}

#[derive(..ApiModel)]
pub struct StreamingChatCompletionToolCall {
    pub function: StreamingChatCompletionResponseFunctionToolCall,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct StreamingChatCompletionResponseFunctionToolCall {
    pub name: String,
    pub arguments: String,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
