use crate::{
    apis::messages::{
        models::{
            api::{request::MessagesRequestBody, response::MessagesResponse},
            lib::options::MessagesOptions,
        },
        non_streaming::non_streaming_messages,
        streaming::streaming_messages,
        virtual_non_streaming::virtual_non_streaming_messages,
    },
    error::Error,
    non_streaming_mode::NonStreamingMode,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub mod virtual_non_streaming;
pub use reqwest::IntoUrl;

pub async fn messages<'a>(
    url: impl IntoUrl,
    body: impl Into<MessagesRequestBody>,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Result<MessagesResponse<'a>, Error> {
    let options = options.into();

    match body.into() {
        MessagesRequestBody::NonStreaming(body) => {
            let mode = options
                .as_ref()
                .map(|o| o.non_streaming_mode)
                .unwrap_or_default();

            match mode {
                NonStreamingMode::Real => non_streaming_messages(url, body, options).await,
                NonStreamingMode::Virtual => {
                    virtual_non_streaming_messages(url, body, options).await
                }
            }
            .map(MessagesResponse::NonStreaming)
        }
        MessagesRequestBody::Streaming(body) => streaming_messages(url, body, options)
            .await
            .map(MessagesResponse::Streaming),
    }
}
