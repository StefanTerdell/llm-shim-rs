use crate::{
    apis::chat_completion::models::api::response::streaming::StreamingChatCompletionChunk,
    error::Error, stats::StreamStats,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingChatCompletionEvent {
    Done {
        stats: StreamStats,
    },
    Chunk {
        chunk: StreamingChatCompletionChunk,
    },
    ChunkError {
        stats: StreamStats,
        chunk: StreamingChatCompletionChunk,
    },
}

pub type StreamingChatCompletionResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingChatCompletionEvent, Error>>>>;
