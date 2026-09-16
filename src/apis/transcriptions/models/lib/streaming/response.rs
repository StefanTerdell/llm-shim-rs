use crate::{
    apis::transcriptions::models::api::response::streaming::TranscriptionsStreamEvent,
    error::Error, stats::StreamStats,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingTranscriptionsEvent {
    Done {
        stats: StreamStats,
    },
    Event {
        event: TranscriptionsStreamEvent,
    },
    EventError {
        stats: StreamStats,
        event: TranscriptionsStreamEvent,
    },
}

pub type StreamingTranscriptionsResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingTranscriptionsEvent, Error>>>>;
