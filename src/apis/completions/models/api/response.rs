use indexmap::IndexMap;
use serde_json::Value;
use stefans_utils::literal_str;

use crate::{
    apis::completions::models::{
        api::common::{CompletionChoice, CompletionUsage},
        lib::streaming::response::StreamingCompletionResponse,
    },
    stats::StreamStats,
};

#[allow(clippy::large_enum_variant)]
pub enum CompletionResponse<'a> {
    NonStreaming(NonStreamingCompletionResponse),
    Streaming(StreamingCompletionResponse<'a>),
}

pub struct NonStreamingCompletionResponse {
    pub body: NonStreamingCompletionResponseBody,
    pub stats: Option<StreamStats>,
    pub error: Option<Value>,
}

literal_str!(CompletionObjectLabel = "text_completion");

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingCompletionResponseBody {
    #[serde(default)]
    pub object: CompletionObjectLabel,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<CompletionChoice>,
    pub usage: Option<CompletionUsage>,
    pub error: Option<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingCompletionChunk {
    pub error: Option<Value>,
    pub choices: Option<Vec<CompletionChoice>>,
    pub usage: Option<CompletionUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
