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
    pub data: Vec<EmbeddingsData>,
    pub model: Option<String>,
    pub usage: Option<EmbeddingsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct EmbeddingsData {
    pub index: u32,
    pub embedding: Embedding,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum Embedding {
    Floats(Vec<f32>),
    Base64(String),
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct EmbeddingsUsage {
    pub prompt_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
