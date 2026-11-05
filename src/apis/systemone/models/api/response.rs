use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct SystemOneResponse {
    pub body: SystemOneResponseBody,
    pub stats: StreamStats,
}

/// `answers`, `routing`, `id` and anything else pass through untouched in
/// `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct SystemOneResponseBody {
    pub usage: Option<SystemOneUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct SystemOneUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
