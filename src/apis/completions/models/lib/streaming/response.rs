use crate::{
    apis::completions::models::api::response::StreamingCompletionChunk, error::Error,
    stats::StreamStats,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingCompletionEvent {
    Done {
        stats: StreamStats,
    },
    Chunk {
        chunk: StreamingCompletionChunk,
    },
    ChunkError {
        stats: StreamStats,
        chunk: StreamingCompletionChunk,
    },
}

pub type StreamingCompletionResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingCompletionEvent, Error>>>>;
