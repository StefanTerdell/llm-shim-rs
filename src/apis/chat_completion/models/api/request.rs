pub mod common;
pub mod non_streaming;
pub mod streaming;

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
