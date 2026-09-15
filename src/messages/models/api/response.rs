pub mod non_streaming;
pub mod streaming;

use crate::messages::models::{
    api::response::non_streaming::NonStreamingMessagesResponse,
    lib::streaming::response::StreamingMessagesResponse,
};

#[allow(clippy::large_enum_variant)]
pub enum MessagesResponse<'a> {
    NonStreaming(NonStreamingMessagesResponse),
    Streaming(StreamingMessagesResponse<'a>),
}
