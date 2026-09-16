pub mod non_streaming;
pub mod streaming;

use crate::transcriptions::models::{
    api::response::non_streaming::NonStreamingTranscriptionsResponse,
    lib::streaming::response::StreamingTranscriptionsResponse,
};

#[allow(clippy::large_enum_variant)]
pub enum TranscriptionsResponse<'a> {
    NonStreaming(NonStreamingTranscriptionsResponse),
    Streaming(StreamingTranscriptionsResponse<'a>),
}
