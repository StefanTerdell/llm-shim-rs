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

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct ResponseBody {
    pub id: Option<String>,
    #[serde(default, with = "serde_with::rust::double_option")]
    #[schemars(with = "Option<Option<String>>")]
    pub status: Option<Option<String>>,
    pub model: Option<String>,
    #[serde(default)]
    pub output: Vec<OutputItem>,
    #[serde(default, with = "serde_with::rust::double_option")]
    #[schemars(with = "Option<Option<ResponsesUsage>>")]
    pub usage: Option<Option<ResponsesUsage>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    #[schemars(with = "Option<Option<Value>>")]
    pub error: Option<Option<Value>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip(v: Value) -> Value {
        serde_json::to_value(serde_json::from_value::<ResponseBody>(v).unwrap()).unwrap()
    }

    #[test]
    fn null_and_absent_fields_round_trip_byte_faithfully() {
        let with_nulls = json!({"id": "r", "object": "response", "status": "in_progress", "output": [], "usage": null, "error": null});
        assert_eq!(round_trip(with_nulls.clone()), with_nulls);

        let without = json!({"id": "r", "output": []});
        assert_eq!(round_trip(without.clone()), without);

        let with_values = json!({"id": "r", "status": "completed", "output": [], "usage": {"input_tokens": 1, "output_tokens": 2, "total_tokens": 3}, "error": {"code": "x"}});
        assert_eq!(round_trip(with_values.clone()), with_values);
    }
}
