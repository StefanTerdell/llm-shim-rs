use crate::{
    error::Error,
    responses::{
        models::{
            api::{request::ResponsesRequestBody, response::ResponsesResponse},
            lib::options::ResponsesOptions,
        },
        non_streaming::non_streaming_responses,
        streaming::streaming_responses,
    },
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub use reqwest::IntoUrl;

pub async fn responses<'a>(
    url: impl IntoUrl,
    body: impl Into<ResponsesRequestBody>,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Result<ResponsesResponse<'a>, Error> {
    match body.into() {
        ResponsesRequestBody::NonStreaming(body) => non_streaming_responses(url, body, options)
            .await
            .map(ResponsesResponse::NonStreaming),
        ResponsesRequestBody::Streaming(body) => streaming_responses(url, body, options)
            .await
            .map(ResponsesResponse::Streaming),
    }
}
