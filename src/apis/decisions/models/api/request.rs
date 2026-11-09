use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

/// Body of a `/v1/decisions` call: an ordered list of typed questions
/// (`predicate`, `choice` or `score`) evaluated against a shared `input`,
/// which is a string or a list of messages and tool call items.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsRequestBody {
    pub model: Option<String>,
    pub input: Value,
    /// Question definitions (`type`, `name`, `instructions`, `choices`,
    /// `levels`). Answers come back in the same order.
    pub questions: Vec<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl DecisionsRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}
