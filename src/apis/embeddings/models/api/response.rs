use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct EmbeddingsResponse {
    pub body: EmbeddingsResponseBody,
    pub stats: StreamStats,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct EmbeddingsResponseBody {
    pub usage: Option<EmbeddingsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct EmbeddingsUsage {
    pub prompt_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
