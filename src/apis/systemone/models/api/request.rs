use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    apis::decisions::models::api::request::{
        DecisionsChoice, DecisionsLevel, DecisionsQuestion, DecisionsRequestBody,
    },
    error::Error,
};

/// Body of a `/v1/systemone` call: typed questions (`choice`, `score` or
/// `noul`) asked about `state`, which is a string, an object or a list of turns.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct SystemOneRequestBody {
    pub model: Option<String>,
    pub state: Value,
    /// Question id to definition.
    pub questions: IndexMap<String, SystemOneQuestion>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl SystemOneRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}

/// `instructions` and anything else pass through in `additional_properties`.
/// Criteria of any other shape, such as rubric objects, make the question
/// [`SystemOneQuestion::Other`].
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum SystemOneQuestion {
    #[serde(rename = "noul")]
    Noul {
        /// Missing and `null` both mean none.
        criteria: Option<Value>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "choice")]
    Choice {
        criteria: SystemOneChoiceCriteria,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "score")]
    Score {
        /// Level descriptions, index 0 first.
        criteria: Vec<String>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum SystemOneChoiceCriteria {
    /// Option to description; a missing or empty description means none.
    Described(IndexMap<String, Option<String>>),
    Listed(Vec<String>),
}

/// A score level's description, when it has one, is sent along with its
/// label as `label: description`, since SystemOne only takes one text per
/// level.
impl TryFrom<DecisionsRequestBody> for SystemOneRequestBody {
    type Error = Error;

    fn try_from(body: DecisionsRequestBody) -> Result<Self, Self::Error> {
        let mut questions = IndexMap::with_capacity(body.questions.len());

        for question in body.questions {
            let (name, question) = match question {
                DecisionsQuestion::Predicate {
                    name,
                    additional_properties,
                } => (
                    name,
                    SystemOneQuestion::Noul {
                        criteria: None,
                        additional_properties,
                    },
                ),
                DecisionsQuestion::Choice {
                    name,
                    choices,
                    additional_properties,
                } => (
                    name,
                    SystemOneQuestion::Choice {
                        criteria: choices.into(),
                        additional_properties,
                    },
                ),
                DecisionsQuestion::Score {
                    name,
                    levels,
                    additional_properties,
                } => (
                    name,
                    SystemOneQuestion::Score {
                        criteria: levels.into_iter().map(level_criterion).collect(),
                        additional_properties,
                    },
                ),
                DecisionsQuestion::Other(_) => {
                    return Err(Error::Translation(
                        "a question has no SystemOne equivalent".to_string(),
                    ));
                }
            };

            if questions.contains_key(&name) {
                return Err(Error::Translation(format!(
                    "question name '{name}' is used more than once"
                )));
            }
            questions.insert(name, question);
        }

        Ok(Self {
            model: body.model,
            state: body.input,
            questions,
            additional_properties: body.additional_properties,
        })
    }
}

/// Choices with any description become a value-to-description map; choices
/// without any stay a plain list.
impl From<Vec<DecisionsChoice>> for SystemOneChoiceCriteria {
    fn from(choices: Vec<DecisionsChoice>) -> Self {
        if choices.iter().all(|choice| choice.description.is_none()) {
            return Self::Listed(choices.into_iter().map(|choice| choice.value).collect());
        }

        Self::Described(
            choices
                .into_iter()
                .map(|choice| (choice.value, choice.description))
                .collect(),
        )
    }
}

fn level_criterion(level: DecisionsLevel) -> String {
    match level.description {
        Some(description) => format!("{}: {description}", level.label),
        None => level.label,
    }
}
