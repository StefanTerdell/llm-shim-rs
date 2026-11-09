use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct DecisionsResponse {
    pub body: DecisionsResponseBody,
    pub stats: StreamStats,
}

/// `answers`, `model` and anything else pass through untouched in
/// `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsResponseBody {
    pub usage: Option<DecisionsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

/// `total_tokens`, `input_tokens_details` and `output_tokens_details` pass
/// through in `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct DecisionsUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
