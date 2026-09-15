use crate::{
    chat_completion::{
        models::{
            api::{request::ChatCompletionRequestBody, response::ChatCompletionResponse},
            lib::options::ChatCompletionOptions,
        },
        non_streaming::non_streaming_chat_completion,
        streaming::streaming_chat_completion,
    },
    error::Error,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub use reqwest::IntoUrl;

pub async fn chat_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<ChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions<'a>>>,
) -> Result<ChatCompletionResponse<'a>, Error> {
    match body.into() {
        ChatCompletionRequestBody::NonStreaming(body) => {
            non_streaming_chat_completion(url, body, options)
                .await
                .map(ChatCompletionResponse::NonStreaming)
        }
        ChatCompletionRequestBody::Streaming(body) => streaming_chat_completion(url, body, options)
            .await
            .map(ChatCompletionResponse::Streaming),
    }
}
