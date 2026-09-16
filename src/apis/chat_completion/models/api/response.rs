pub mod common;
pub mod non_streaming;
pub mod streaming;

use crate::apis::chat_completion::models::{
    api::response::non_streaming::NonStreamingChatCompletionResponse,
    lib::streaming::response::StreamingChatCompletionResponse,
};

#[allow(clippy::large_enum_variant)]
pub enum ChatCompletionResponse<'a> {
    NonStreaming(NonStreamingChatCompletionResponse),
    Streaming(StreamingChatCompletionResponse<'a>),
}
