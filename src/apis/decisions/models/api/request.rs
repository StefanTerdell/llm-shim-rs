use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    apis::systemone::models::api::request::{
        SystemOneChoiceCriteria, SystemOneQuestion, SystemOneRequestBody,
    },
    error::Error,
};

/// Body of a `/v1/decisions` call: an ordered list of typed questions
/// (`predicate`, `choice` or `score`) evaluated against a shared `input`,
/// which is a string or a list of messages and tool call items.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsRequestBody {
    pub model: Option<String>,
    pub input: Value,
    /// Answers come back in the same order.
    pub questions: Vec<DecisionsQuestion>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl DecisionsRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}

/// `instructions` and anything else pass through in `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum DecisionsQuestion {
    #[serde(rename = "predicate")]
    Predicate {
        name: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "choice")]
    Choice {
        name: String,
        choices: Vec<DecisionsChoice>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "score")]
    Score {
        name: String,
        /// Lowest first.
        levels: Vec<DecisionsLevel>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsChoice {
    pub value: String,
    pub description: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsLevel {
    pub label: String,
    pub description: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

/// A SystemOne `state` object becomes its JSON text, since Decisions `input`
/// is a string or a list. A `noul` question's `criteria`, if any, passes
/// through on the predicate.
impl TryFrom<SystemOneRequestBody> for DecisionsRequestBody {
    type Error = Error;

    fn try_from(body: SystemOneRequestBody) -> Result<Self, Self::Error> {
        let input = match body.state {
            state @ (Value::String(_) | Value::Array(_)) => state,
            state => Value::String(state.to_string()),
        };

        let questions = body
            .questions
            .into_iter()
            .map(|(name, question)| match question {
                SystemOneQuestion::Noul {
                    criteria,
                    mut additional_properties,
                } => {
                    if let Some(criteria) = criteria {
                        additional_properties.insert("criteria".to_string(), criteria);
                    }
                    Ok(DecisionsQuestion::Predicate {
                        name,
                        additional_properties,
                    })
                }
                SystemOneQuestion::Choice {
                    criteria,
                    additional_properties,
                } => Ok(DecisionsQuestion::Choice {
                    name,
                    choices: criteria.into(),
                    additional_properties,
                }),
                SystemOneQuestion::Score {
                    criteria,
                    additional_properties,
                } => Ok(DecisionsQuestion::Score {
                    name,
                    levels: criteria
                        .into_iter()
                        .map(|label| DecisionsLevel {
                            label,
                            description: None,
                            additional_properties: IndexMap::new(),
                        })
                        .collect(),
                    additional_properties,
                }),
                SystemOneQuestion::Other(_) => Err(Error::Translation(format!(
                    "question '{name}' has no Decisions equivalent"
                ))),
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            model: body.model,
            input,
            questions,
            additional_properties: body.additional_properties,
        })
    }
}

impl From<SystemOneChoiceCriteria> for Vec<DecisionsChoice> {
    fn from(criteria: SystemOneChoiceCriteria) -> Self {
        let choice = |value, description| DecisionsChoice {
            value,
            description,
            additional_properties: IndexMap::new(),
        };

        match criteria {
            SystemOneChoiceCriteria::Described(described) => described
                .into_iter()
                .map(|(value, description)| {
                    choice(
                        value,
                        description.filter(|description| !description.is_empty()),
                    )
                })
                .collect(),
            SystemOneChoiceCriteria::Listed(values) => values
                .into_iter()
                .map(|value| choice(value, None))
                .collect(),
        }
    }
}
