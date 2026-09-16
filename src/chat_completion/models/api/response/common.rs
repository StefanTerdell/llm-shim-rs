use std::ops::Add;

use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    chat_completion::models::api::common::CommonChatCompletionMessage, traits::or_merge::OrMerge,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionResponseMessage {
    pub content: Option<String>,
    #[serde(flatten)]
    pub common: CommonChatCompletionMessage,
}

impl Add for ChatCompletionResponseMessage {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            content: self.content.or_merge(rhs.content),
            common: self.common + rhs.common,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonChatCompletionChoice {
    pub index: u32,
    pub logprobs: Option<IndexMap<String, Vec<Value>>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Add for CommonChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            index: rhs.index,
            logprobs: self.logprobs.or_merge(rhs.logprobs),
            additional_properties: self
                .additional_properties
                .into_iter()
                .chain(rhs.additional_properties)
                .collect(),
        }
    }
}
