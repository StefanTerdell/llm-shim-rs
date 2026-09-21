use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonResponsesRequestBody {
    pub model: Option<String>,
    pub input: ResponsesInput,
    pub instructions: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl CommonResponsesRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ResponsesInput {
    Text(String),
    Items(Vec<Value>),
}
