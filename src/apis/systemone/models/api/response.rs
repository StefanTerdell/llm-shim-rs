use indexmap::IndexMap;
use serde_json::Value;

use crate::{
    apis::decisions::models::api::{
        request::DecisionsQuestion,
        response::{
            DecisionsAnswer, DecisionsChoiceProbability, DecisionsLevelProbability,
            DecisionsResponseBody, DecisionsUsage,
        },
    },
    error::Error,
    stats::StreamStats,
};

pub struct SystemOneResponse {
    pub body: SystemOneResponseBody,
    pub stats: StreamStats,
}

/// `routing`, `id` and anything else pass through untouched in
/// `additional_properties`.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct SystemOneResponseBody {
    /// Question id to answer.
    pub answers: Option<IndexMap<String, SystemOneAnswer>>,
    pub usage: Option<SystemOneUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

/// `confidence` is Laya's own normalized-entropy confidence;
/// `x_jev_confidence` is the one Jev's formula gives.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum SystemOneAnswer {
    #[serde(rename = "noul")]
    Noul {
        noul: Option<Value>,
        confidence: Option<Value>,
        x_jev_confidence: Option<Value>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "choice")]
    Choice {
        choice: Option<Value>,
        /// Option to probability.
        probabilities: Option<IndexMap<String, Value>>,
        confidence: Option<Value>,
        x_jev_confidence: Option<Value>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "score")]
    Score {
        score: Option<Value>,
        /// Level index to the level's criterion.
        legend: Option<IndexMap<String, String>>,
        /// Level index to probability.
        probabilities: Option<IndexMap<String, Value>>,
        confidence: Option<Value>,
        x_jev_confidence: Option<Value>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct SystemOneUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl From<DecisionsUsage> for SystemOneUsage {
    fn from(usage: DecisionsUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            additional_properties: usage.additional_properties,
        }
    }
}

impl SystemOneResponseBody {
    /// Rewrites a SystemOne answer as the answer to `questions`, the Decisions
    /// request it was translated from, in that request's question order and
    /// with its choice values and level labels.
    ///
    /// `x_jev_confidence` becomes the Decisions `confidence`, since Jev's
    /// formula is the one a Decisions client expects; Laya's own `confidence`
    /// is used only when there is no `x_jev_confidence`.
    pub fn into_decisions(
        self,
        questions: &[DecisionsQuestion],
    ) -> Result<DecisionsResponseBody, Error> {
        let mut answers = self.answers.unwrap_or_default();

        let answers = questions
            .iter()
            .map(|question| {
                let name = match question {
                    DecisionsQuestion::Predicate { name, .. }
                    | DecisionsQuestion::Choice { name, .. }
                    | DecisionsQuestion::Score { name, .. } => name,
                    DecisionsQuestion::Other(_) => {
                        return Err(Error::Translation(
                            "a question has no SystemOne equivalent".to_string(),
                        ));
                    }
                };
                let answer = answers.shift_remove(name).ok_or_else(|| {
                    Error::Translation(format!(
                        "the provider returned no answer for question '{name}'"
                    ))
                })?;

                answer_to_decisions(question, name.clone(), answer)
            })
            .collect::<Result<_, _>>()?;

        Ok(DecisionsResponseBody {
            object: Some("decisions".to_string()),
            answers: Some(answers),
            usage: self.usage.map(Into::into),
            additional_properties: self.additional_properties,
        })
    }
}

fn answer_to_decisions(
    question: &DecisionsQuestion,
    name: String,
    answer: SystemOneAnswer,
) -> Result<DecisionsAnswer, Error> {
    Ok(match (question, answer) {
        (
            DecisionsQuestion::Predicate { .. },
            SystemOneAnswer::Noul {
                noul,
                confidence,
                x_jev_confidence,
                additional_properties,
            },
        ) => DecisionsAnswer::Predicate {
            name,
            probability: noul,
            confidence: x_jev_confidence.or(confidence),
            additional_properties,
        },
        (
            DecisionsQuestion::Choice { choices, .. },
            SystemOneAnswer::Choice {
                choice,
                probabilities,
                confidence,
                x_jev_confidence,
                additional_properties,
            },
        ) => DecisionsAnswer::Choice {
            name,
            choice,
            confidence: x_jev_confidence.or(confidence),
            probabilities: probabilities.map(|probabilities| {
                choices
                    .iter()
                    .filter_map(|choice| {
                        probabilities.get(&choice.value).map(|probability| {
                            DecisionsChoiceProbability {
                                value: choice.value.clone(),
                                probability: Some(probability.clone()),
                                additional_properties: IndexMap::new(),
                            }
                        })
                    })
                    .collect()
            }),
            additional_properties,
        },
        (
            DecisionsQuestion::Score { levels, .. },
            SystemOneAnswer::Score {
                score,
                probabilities,
                confidence,
                x_jev_confidence,
                additional_properties,
                ..
            },
        ) => DecisionsAnswer::Score {
            name,
            score,
            confidence: x_jev_confidence.or(confidence),
            probabilities: probabilities.map(|probabilities| {
                (0u32..)
                    .zip(levels)
                    .filter_map(|(value, level)| {
                        probabilities.get(&value.to_string()).map(|probability| {
                            DecisionsLevelProbability {
                                label: Some(level.label.clone()),
                                value,
                                probability: Some(probability.clone()),
                                additional_properties: IndexMap::new(),
                            }
                        })
                    })
                    .collect()
            }),
            additional_properties,
        },
        _ => {
            return Err(Error::Translation(format!(
                "the provider's answer to question '{name}' does not match its type"
            )));
        }
    })
}
