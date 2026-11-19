use crate::{
    apis::completions::{
        models::{
            api::{request::CompletionRequestBody, response::CompletionResponse},
            lib::options::CompletionsOptions,
        },
        non_streaming::non_streaming_completion,
        streaming::streaming_completion,
        virtual_non_streaming::virtual_non_streaming_completion,
    },
    error::Error,
    non_streaming_mode::NonStreamingMode,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub mod virtual_non_streaming;
pub use reqwest::IntoUrl;

pub async fn completion<'a>(
    url: impl IntoUrl,
    body: impl Into<CompletionRequestBody>,
    options: impl Into<Option<CompletionsOptions<'a>>>,
) -> Result<CompletionResponse<'a>, Error> {
    let options = options.into();

    match body.into() {
        CompletionRequestBody::NonStreaming(body) => {
            let mode = options
                .as_ref()
                .map(|o| o.non_streaming_mode)
                .unwrap_or_default();

            match mode {
                NonStreamingMode::Real => non_streaming_completion(url, body, options).await,
                NonStreamingMode::Virtual => {
                    virtual_non_streaming_completion(url, body, options).await
                }
            }
            .map(CompletionResponse::NonStreaming)
        }
        CompletionRequestBody::Streaming(body) => streaming_completion(url, body, options)
            .await
            .map(CompletionResponse::Streaming),
    }
}
