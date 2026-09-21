pub mod common;
pub mod non_streaming;
pub mod streaming;

use std::fmt::Display;

use crate::apis::chat_completion::models::api::request::{
    non_streaming::NonStreamingChatCompletionRequestBody,
    streaming::StreamingChatCompletionRequestBody,
};

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ChatCompletionRequestBody {
    NonStreaming(NonStreamingChatCompletionRequestBody),
    Streaming(StreamingChatCompletionRequestBody),
}

impl ChatCompletionRequestBody {
    pub fn with_model(self, model: Option<impl Display>) -> Self {
        match self {
            Self::NonStreaming(body) => Self::NonStreaming(body.with_model(model)),
            Self::Streaming(body) => Self::Streaming(body.with_model(model)),
        }
    }
}

impl From<ChatCompletionRequestBody> for NonStreamingChatCompletionRequestBody {
    fn from(value: ChatCompletionRequestBody) -> Self {
        match value {
            ChatCompletionRequestBody::NonStreaming(x) => x,
            ChatCompletionRequestBody::Streaming(x) => x.into(),
        }
    }
}

impl From<ChatCompletionRequestBody> for StreamingChatCompletionRequestBody {
    fn from(value: ChatCompletionRequestBody) -> Self {
        match value {
            ChatCompletionRequestBody::Streaming(x) => x,
            ChatCompletionRequestBody::NonStreaming(x) => x.into(),
        }
    }
}
