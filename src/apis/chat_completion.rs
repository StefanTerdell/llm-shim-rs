use crate::{
    apis::chat_completion::{
        models::{
            api::{request::ChatCompletionRequestBody, response::ChatCompletionResponse},
            lib::options::ChatCompletionOptions,
        },
        non_streaming::non_streaming_chat_completion,
        streaming::streaming_chat_completion,
        virtual_non_streaming::virtual_non_streaming_chat_completion,
    },
    error::Error,
    non_streaming_mode::NonStreamingMode,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub mod virtual_non_streaming;
pub use reqwest::IntoUrl;

pub async fn chat_completion<'a>(
    url: impl IntoUrl,
    body: impl Into<ChatCompletionRequestBody>,
    options: impl Into<Option<ChatCompletionOptions<'a>>>,
) -> Result<ChatCompletionResponse<'a>, Error> {
    let options = options.into();

    match body.into() {
        ChatCompletionRequestBody::NonStreaming(body) => {
            let mode = options
                .as_ref()
                .map(|o| o.non_streaming_mode)
                .unwrap_or_default();

            match mode {
                NonStreamingMode::Real => non_streaming_chat_completion(url, body, options).await,
                NonStreamingMode::Virtual => {
                    virtual_non_streaming_chat_completion(url, body, options).await
                }
            }
            .map(ChatCompletionResponse::NonStreaming)
        }
        ChatCompletionRequestBody::Streaming(body) => streaming_chat_completion(url, body, options)
            .await
            .map(ChatCompletionResponse::Streaming),
    }
}
