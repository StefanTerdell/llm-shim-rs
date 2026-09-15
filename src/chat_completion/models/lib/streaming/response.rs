use crate::{
    chat_completion::models::{
        api::response::streaming::StreamingChatCompletionChunk,
        lib::common::stats::ChatCompletionStats,
    },
    error::Error,
};

use std::pin::Pin;
use tokio_stream::Stream;

pub enum StreamingChatCompletionEvent {
    Done {
        stats: ChatCompletionStats,
    },
    Chunk {
        chunk: StreamingChatCompletionChunk,
    },
    ChunkError {
        stats: ChatCompletionStats,
        chunk: StreamingChatCompletionChunk,
    },
}

pub type StreamingChatCompletionResponse<'a> =
    Pin<Box<dyn 'a + Send + Stream<Item = Result<StreamingChatCompletionEvent, Error>>>>;
