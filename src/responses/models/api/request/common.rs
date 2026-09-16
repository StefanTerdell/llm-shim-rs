use indexmap::IndexMap;
use serde_json::Value;

#[derive(..ApiModel)]
pub struct CommonResponsesRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub input: ResponsesInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ResponsesInput {
    Text(String),
    Items(Vec<Value>),
}
