use std::ops::Add;

use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    apis::chat_completion::models::api::common::CommonChatCompletionMessage,
    traits::{merge::Merge, or_merge::OrMerge},
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionResponseMessage {
    /// `None` is absent, `Some(None)` is an explicit `null`, so both round-trip.
    #[serde(default, with = "serde_with::rust::double_option")]
    #[schemars(with = "Option<Option<String>>")]
    pub content: Option<Option<String>>,
    #[serde(flatten)]
    pub common: CommonChatCompletionMessage,
}

impl ChatCompletionResponseMessage {
    pub fn content(&self) -> Option<&str> {
        self.content.as_ref().and_then(|c| c.as_deref())
    }

    pub fn content_mut(&mut self) -> Option<&mut String> {
        self.content.as_mut().and_then(|c| c.as_mut())
    }

    pub fn content_or_default(&mut self) -> &mut String {
        self.content.get_or_insert_default().get_or_insert_default()
    }
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
    pub logprobs: Option<IndexMap<String, Option<Vec<Value>>>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Add for CommonChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            index: rhs.index,
            logprobs: self.logprobs.or_merge(rhs.logprobs),
            additional_properties: self.additional_properties.merge(rhs.additional_properties),
        }
    }
}
