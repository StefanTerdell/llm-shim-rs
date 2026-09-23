use crate::{
    apis::responses::{
        models::{
            api::{request::ResponsesRequestBody, response::ResponsesResponse},
            lib::options::ResponsesOptions,
        },
        non_streaming::non_streaming_responses,
        streaming::streaming_responses,
        virtual_non_streaming::virtual_non_streaming_responses,
    },
    error::Error,
    non_streaming_mode::NonStreamingMode,
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub mod virtual_non_streaming;
pub use reqwest::IntoUrl;

pub async fn responses<'a>(
    url: impl IntoUrl,
    body: impl Into<ResponsesRequestBody>,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Result<ResponsesResponse<'a>, Error> {
    let options = options.into();

    match body.into() {
        ResponsesRequestBody::NonStreaming(body) => {
            let mode = options
                .as_ref()
                .map(|o| o.non_streaming_mode)
                .unwrap_or_default();

            match mode {
                NonStreamingMode::Real => non_streaming_responses(url, body, options).await,
                NonStreamingMode::Virtual => {
                    virtual_non_streaming_responses(url, body, options).await
                }
            }
            .map(ResponsesResponse::NonStreaming)
        }
        ResponsesRequestBody::Streaming(body) => streaming_responses(url, body, options)
            .await
            .map(ResponsesResponse::Streaming),
    }
}
