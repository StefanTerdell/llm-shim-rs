use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    responses::models::api::common::{OutputItem, ResponsesUsage},
    stats::StreamStats,
};

pub struct NonStreamingResponsesResponse {
    pub body: ResponseBody,
    pub stats: Option<StreamStats>,
    pub error: Option<Value>,
}

#[derive(..ApiModel, Default)]
pub struct ResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub output: Vec<OutputItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
