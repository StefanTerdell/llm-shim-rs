use crate::{
    error::Error, responses::models::api::response::streaming::ResponsesStreamEvent,
    stats::StreamStats,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingResponsesEvent {
    Done {
        stats: StreamStats,
    },
    Event {
        event: ResponsesStreamEvent,
    },
    EventError {
        stats: StreamStats,
        event: ResponsesStreamEvent,
    },
}

pub type StreamingResponsesResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingResponsesEvent, Error>>>>;
