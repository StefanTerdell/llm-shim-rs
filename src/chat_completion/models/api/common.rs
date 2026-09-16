use crate::{stats::StreamStats, traits::or_merge::OrMerge};

use indexmap::IndexMap;
use serde_json::Value;
use std::ops::Add;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonChatCompletionMessage {
    #[serialize_always]
    pub role: Option<String>,
    pub tool_calls: Option<Vec<ChatCompletionRequestMessageToolCall>>,
    pub reasoning_content: Option<String>,
    pub reasoning: Option<String>,
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Add for CommonChatCompletionMessage {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            role: rhs.role.or(self.role),
            tool_calls: self.tool_calls.or_merge(rhs.tool_calls),
            reasoning_content: self.reasoning_content.or_merge(rhs.reasoning_content),
            reasoning: self.reasoning.or_merge(rhs.reasoning),
            name: rhs.name.or(self.name),
            additional_properties: self
                .additional_properties
                .into_iter()
                .chain(rhs.additional_properties)
                .collect(),
        }
    }
}

#[derive(..ApiModel)]
pub struct ChatCompletionRequestMessageToolCall {
    pub function: ChatCompletionRequestMessageFunctionToolCall,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct ChatCompletionRequestMessageFunctionToolCall {
    pub name: String,
    pub arguments: String,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel, Default)]
pub struct ChatCompletionUsage {
    completion_tokens: u32,
    prompt_tokens: u32,
    total_tokens: u32,
    #[serde(flatten)]
    additional_properties: IndexMap<String, Value>,
}

impl From<&StreamStats> for ChatCompletionUsage {
    fn from(value: &StreamStats) -> Self {
        Self {
            completion_tokens: value.output_tokens,
            prompt_tokens: value.input_tokens,
            total_tokens: value.input_tokens + value.output_tokens,
            additional_properties: Default::default(),
        }
    }
}

impl ChatCompletionUsage {
    pub fn with_completion_tokens(mut self, completion_tokens: u32) -> Self {
        self.completion_tokens = completion_tokens;
        self.total_tokens = self.completion_tokens + self.prompt_tokens;
        self
    }

    pub fn with_prompt_tokens(mut self, prompt_tokens: u32) -> Self {
        self.prompt_tokens = prompt_tokens;
        self.total_tokens = self.prompt_tokens + self.completion_tokens;
        self
    }

    pub fn completion_tokens(&self) -> u32 {
        self.completion_tokens
    }

    pub fn prompt_tokens(&self) -> u32 {
        self.prompt_tokens
    }

    pub fn total_tokens(&self) -> u32 {
        self.total_tokens
    }
}
