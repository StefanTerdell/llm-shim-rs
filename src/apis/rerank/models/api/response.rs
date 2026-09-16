use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct RerankResponse {
    pub body: RerankResponseBody,
    pub stats: StreamStats,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct RerankResponseBody {
    pub usage: Option<RerankUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct RerankUsage {
    pub total_tokens: Option<u32>,
    pub prompt_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
