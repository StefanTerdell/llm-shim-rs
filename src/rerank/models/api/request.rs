use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct RerankRequestBody {
    pub model: String,
    pub query: Value,
    pub documents: Vec<Value>,
    pub top_n: Option<u32>,
    pub return_documents: Option<bool>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
