use crate::{
    stats::StreamStats,
    traits::{merge::Merge, or_merge::OrMerge},
};

use indexmap::IndexMap;
use serde_json::Value;
use std::ops::Add;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonChatCompletionMessage {
    pub tool_calls: Option<Vec<ChatCompletionRequestMessageToolCall>>,
    pub reasoning_content: Option<String>,
    pub reasoning: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Add for CommonChatCompletionMessage {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            tool_calls: self.tool_calls.or_merge(rhs.tool_calls),
            reasoning_content: self.reasoning_content.or_merge(rhs.reasoning_content),
            reasoning: self.reasoning.or_merge(rhs.reasoning),
            additional_properties: self.additional_properties.merge(rhs.additional_properties),
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionRequestMessageToolCall {
    pub index: Option<u32>,
    pub function: ChatCompletionRequestMessageFunctionToolCall,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Merge for ChatCompletionRequestMessageToolCall {
    fn merge(self, rhs: Self) -> Self {
        Self {
            index: rhs.index.or(self.index),
            function: self.function.merge(rhs.function),
            additional_properties: self.additional_properties.merge(rhs.additional_properties),
        }
    }
}

impl Merge for Vec<ChatCompletionRequestMessageToolCall> {
    fn merge(mut self, rhs: Self) -> Self {
        for delta in rhs {
            let position = delta
                .index
                .and_then(|index| self.iter().position(|call| call.index == Some(index)));

            match position {
                Some(position) => {
                    let current = self.remove(position);
                    self.insert(position, current.merge(delta));
                }
                None => self.push(delta),
            }
        }

        self
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionRequestMessageFunctionToolCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl Merge for ChatCompletionRequestMessageFunctionToolCall {
    fn merge(self, rhs: Self) -> Self {
        Self {
            name: self.name.or_merge(rhs.name),
            arguments: self.arguments.or_merge(rhs.arguments),
            additional_properties: self.additional_properties.merge(rhs.additional_properties),
        }
    }
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
