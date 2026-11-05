use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

/// Body of a `/v1/systemone` call: typed questions (`choice`, `score` or
/// `noul`) asked about `state`, which is a string, an object or a list of turns.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct SystemOneRequestBody {
    pub model: Option<String>,
    pub state: Value,
    /// Question id to definition (`type`, `instructions`, `criteria`).
    pub questions: IndexMap<String, Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl SystemOneRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}
