use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;
use stefans_utils::literals::True;

use crate::apis::chat_completion::models::api::request::{
    ChatCompletionRequestBody, common::CommonChatCompletionRequestBody,
    non_streaming::NonStreamingChatCompletionRequestBody,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingChatCompletionRequestBody {
    pub stream: True,
    pub stream_options: Option<StreamingChatCompletionRequestBodyStreamOptions>,
    #[serde(flatten)]
    pub common: CommonChatCompletionRequestBody,
}

impl StreamingChatCompletionRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.common = self.common.with_model(model);
        self
    }
}

impl From<StreamingChatCompletionRequestBody> for NonStreamingChatCompletionRequestBody {
    fn from(value: StreamingChatCompletionRequestBody) -> Self {
        Self {
            stream: None,
            common: value.common,
        }
    }
}

impl From<StreamingChatCompletionRequestBody> for ChatCompletionRequestBody {
    fn from(value: StreamingChatCompletionRequestBody) -> Self {
        Self::Streaming(value)
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingChatCompletionRequestBodyStreamOptions {
    pub include_usage: Option<bool>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
