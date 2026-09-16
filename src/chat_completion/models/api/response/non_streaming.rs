use crate::{
    chat_completion::models::api::{
        common::ChatCompletionUsage,
        response::{
            common::{ChatCompletionResponseMessage, CommonChatCompletionChoice},
            streaming::StreamingChatCompletionChoice,
        },
    },
    traits::reasoning_content_remapping::ReasoningContentRemappingTarget,
};

use crate::stats::StreamStats;
use serde_json::Value;
use std::ops::Add;
use stefans_utils::literal_str;

pub struct NonStreamingChatCompletionResponse {
    pub body: NonStreamingChatCompletionResponseBody,
    pub stats: Option<StreamStats>,
    pub error: Option<Value>,
}

literal_str!(ChatCompletionObjectLabel = "chat.completion");

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingChatCompletionResponseBody {
    pub object: ChatCompletionObjectLabel,
    pub model: Option<String>,
    pub choices: Vec<NonStreamingChatCompletionChoice>,
    pub usage: ChatCompletionUsage,
}

#[derive(..ApiModel)]
pub struct NonStreamingChatCompletionChoice {
    pub message: ChatCompletionResponseMessage,
    #[serde(flatten)]
    pub common: CommonChatCompletionChoice,
}

impl ReasoningContentRemappingTarget for NonStreamingChatCompletionChoice {
    type Inner = ChatCompletionResponseMessage;

    fn index(&self) -> usize {
        self.common.index as usize
    }

    fn inner_mut_opt(&mut self) -> Option<&mut Self::Inner> {
        Some(&mut self.message)
    }
}

impl ReasoningContentRemappingTarget for StreamingChatCompletionChoice {
    type Inner = ChatCompletionResponseMessage;

    fn index(&self) -> usize {
        self.common.index as usize
    }

    fn inner_mut_opt(&mut self) -> Option<&mut Self::Inner> {
        Some(&mut self.delta)
    }
}

impl From<StreamingChatCompletionChoice> for NonStreamingChatCompletionChoice {
    fn from(value: StreamingChatCompletionChoice) -> Self {
        Self {
            message: value.delta,
            common: value.common,
        }
    }
}

impl Add for NonStreamingChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            message: self.message + rhs.message,
            common: self.common + rhs.common,
        }
    }
}

impl Add<StreamingChatCompletionChoice> for NonStreamingChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: StreamingChatCompletionChoice) -> Self::Output {
        self + Self::from(rhs)
    }
}
