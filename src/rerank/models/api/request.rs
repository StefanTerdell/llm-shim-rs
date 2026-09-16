use indexmap::IndexMap;
use serde_json::Value;

#[derive(..ApiModel)]
pub struct RerankRequestBody {
    pub query: Value,
    pub documents: Vec<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
