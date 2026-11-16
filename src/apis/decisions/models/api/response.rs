use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    apis::systemone::models::api::{
        request::SystemOneQuestion,
        response::{SystemOneAnswer, SystemOneResponseBody, SystemOneUsage},
    },
    error::Error,
    stats::StreamStats,
};

pub struct DecisionsResponse {
    pub body: DecisionsResponseBody,
    pub stats: StreamStats,
}

/// `model` and anything else pass through untouched in
/// `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsResponseBody {
    pub object: Option<String>,
    /// In question order.
    pub answers: Option<Vec<DecisionsAnswer>>,
    pub usage: Option<DecisionsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum DecisionsAnswer {
    #[serde(rename = "predicate")]
    Predicate {
        name: String,
        probability: Option<Value>,
        confidence: Option<Value>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "choice")]
    Choice {
        name: String,
        choice: Option<Value>,
        confidence: Option<Value>,
        probabilities: Option<Vec<DecisionsChoiceProbability>>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "score")]
    Score {
        name: String,
        score: Option<Value>,
        confidence: Option<Value>,
        probabilities: Option<Vec<DecisionsLevelProbability>>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsChoiceProbability {
    pub value: String,
    pub probability: Option<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct DecisionsLevelProbability {
    pub label: Option<String>,
    /// The level's index, lowest first.
    pub value: u32,
    pub probability: Option<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

/// `input_tokens_details` and `output_tokens_details` pass through in
/// `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct DecisionsUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl From<SystemOneUsage> for DecisionsUsage {
    fn from(usage: SystemOneUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage
                .input_tokens
                .zip(usage.output_tokens)
                .map(|(input, output)| input.saturating_add(output)),
            additional_properties: usage.additional_properties,
        }
    }
}

impl DecisionsResponseBody {
    /// Rewrites a Decisions answer as the answer to `questions`, the SystemOne
    /// request it was translated from, in that request's question order.
    pub fn into_systemone(
        self,
        questions: &IndexMap<String, SystemOneQuestion>,
    ) -> Result<SystemOneResponseBody, Error> {
        let mut by_name: IndexMap<String, SystemOneAnswer> = self
            .answers
            .unwrap_or_default()
            .into_iter()
            .filter_map(answer_to_systemone)
            .collect();

        let answers = questions
            .keys()
            .map(|name| {
                by_name
                    .shift_remove(name)
                    .map(|answer| (name.clone(), answer))
                    .ok_or_else(|| {
                        Error::Translation(format!(
                            "the provider returned no answer for question '{name}'"
                        ))
                    })
            })
            .collect::<Result<_, _>>()?;

        Ok(SystemOneResponseBody {
            answers: Some(answers),
            usage: self.usage.map(Into::into),
            additional_properties: self.additional_properties,
        })
    }
}

/// The answer's question name and its SystemOne form; `None` for an answer of
/// a type SystemOne has no equivalent for.
fn answer_to_systemone(answer: DecisionsAnswer) -> Option<(String, SystemOneAnswer)> {
    Some(match answer {
        DecisionsAnswer::Predicate {
            name,
            probability,
            confidence,
            additional_properties,
        } => (
            name,
            SystemOneAnswer::Noul {
                noul: probability,
                confidence,
                x_jev_confidence: None,
                additional_properties,
            },
        ),
        DecisionsAnswer::Choice {
            name,
            choice,
            confidence,
            probabilities,
            additional_properties,
        } => (
            name,
            SystemOneAnswer::Choice {
                choice,
                probabilities: probabilities.map(|probabilities| {
                    probabilities
                        .into_iter()
                        .map(|entry| (entry.value, entry.probability.unwrap_or_default()))
                        .collect()
                }),
                confidence,
                x_jev_confidence: None,
                additional_properties,
            },
        ),
        DecisionsAnswer::Score {
            name,
            score,
            confidence,
            probabilities,
            additional_properties,
        } => {
            let probabilities = probabilities.unwrap_or_default();
            (
                name,
                SystemOneAnswer::Score {
                    score,
                    legend: Some(
                        probabilities
                            .iter()
                            .filter_map(|entry| {
                                entry
                                    .label
                                    .clone()
                                    .map(|label| (entry.value.to_string(), label))
                            })
                            .collect(),
                    ),
                    probabilities: Some(
                        probabilities
                            .into_iter()
                            .map(|entry| {
                                (
                                    entry.value.to_string(),
                                    entry.probability.unwrap_or_default(),
                                )
                            })
                            .collect(),
                    ),
                    confidence,
                    x_jev_confidence: None,
                    additional_properties,
                },
            )
        }
        DecisionsAnswer::Other(_) => return None,
    })
}
