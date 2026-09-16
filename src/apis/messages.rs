use crate::{
    apis::messages::{
        models::{
            api::{request::MessagesRequestBody, response::MessagesResponse},
            lib::options::MessagesOptions,
        },
        non_streaming::non_streaming_messages,
        streaming::streaming_messages,
    },
    error::Error,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub use reqwest::IntoUrl;

pub async fn messages<'a>(
    url: impl IntoUrl,
    body: impl Into<MessagesRequestBody>,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Result<MessagesResponse<'a>, Error> {
    match body.into() {
        MessagesRequestBody::NonStreaming(body) => non_streaming_messages(url, body, options)
            .await
            .map(MessagesResponse::NonStreaming),
        MessagesRequestBody::Streaming(body) => streaming_messages(url, body, options)
            .await
            .map(MessagesResponse::Streaming),
    }
}
