use crate::{
    error::Error,
    transcriptions::{
        models::{
            api::{request::TranscriptionsRequest, response::TranscriptionsResponse},
            lib::options::TranscriptionsOptions,
        },
        non_streaming::non_streaming_transcriptions,
        streaming::streaming_transcriptions,
    },
};

pub mod models;
pub mod non_streaming;
pub mod streaming;
pub use reqwest::IntoUrl;

pub async fn transcriptions<'a>(
    url: impl IntoUrl,
    request: TranscriptionsRequest,
    options: impl Into<Option<TranscriptionsOptions<'a>>>,
) -> Result<TranscriptionsResponse<'a>, Error> {
    if request.fields.stream == Some(true) {
        streaming_transcriptions(url, request, options)
            .await
            .map(TranscriptionsResponse::Streaming)
    } else {
        non_streaming_transcriptions(url, request, options)
            .await
            .map(TranscriptionsResponse::NonStreaming)
    }
}
