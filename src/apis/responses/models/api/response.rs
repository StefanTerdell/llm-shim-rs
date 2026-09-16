pub mod non_streaming;
pub mod streaming;

use crate::apis::responses::models::{
    api::response::non_streaming::NonStreamingResponsesResponse,
    lib::streaming::response::StreamingResponsesResponse,
};

#[allow(clippy::large_enum_variant)]
pub enum ResponsesResponse<'a> {
    NonStreaming(NonStreamingResponsesResponse),
    Streaming(StreamingResponsesResponse<'a>),
}
