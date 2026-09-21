use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct EmbeddingsRequestBody {
    pub model: Option<String>,
    pub input: EmbeddingsInput,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl EmbeddingsRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum EmbeddingsInput {
    Text(String),
    Texts(Vec<String>),
    Tokens(Vec<u32>),
    TokenBatches(Vec<Vec<u32>>),
}
