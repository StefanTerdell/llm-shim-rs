use crate::{
    error::Error, messages::models::api::response::streaming::MessagesStreamEvent,
    stats::StreamStats,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingMessagesEvent {
    Done {
        stats: StreamStats,
    },
    Event {
        event: MessagesStreamEvent,
    },
    EventError {
        stats: StreamStats,
        event: MessagesStreamEvent,
    },
}

pub type StreamingMessagesResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingMessagesEvent, Error>>>>;
