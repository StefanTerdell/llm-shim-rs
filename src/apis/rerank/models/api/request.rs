use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct RerankRequestBody {
    pub model: Option<String>,
    pub query: Value,
    pub documents: Vec<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl RerankRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}
