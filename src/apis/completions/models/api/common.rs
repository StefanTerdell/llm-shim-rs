use crate::{stats::StreamStats, traits::merge::Merge};

use indexmap::IndexMap;
use serde_json::Value;

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct CompletionUsage {
    completion_tokens: u32,
    prompt_tokens: u32,
    total_tokens: u32,
    prompt_tokens_details: Option<CompletionPromptTokensDetails>,
    #[serde(flatten)]
    additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct CompletionPromptTokensDetails {
    pub cached_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl From<&StreamStats> for CompletionUsage {
    fn from(value: &StreamStats) -> Self {
        Self {
            completion_tokens: value.output_tokens,
            prompt_tokens: value.input_tokens,
            total_tokens: value.input_tokens + value.output_tokens,
            prompt_tokens_details: (value.cached_input_tokens > 0).then(|| {
                CompletionPromptTokensDetails {
                    cached_tokens: Some(value.cached_input_tokens),
                    additional_properties: Default::default(),
                }
            }),
            additional_properties: Default::default(),
        }
    }
}

impl CompletionUsage {
    pub fn completion_tokens(&self) -> u32 {
        self.completion_tokens
    }

    pub fn prompt_tokens(&self) -> u32 {
        self.prompt_tokens
    }

    pub fn total_tokens(&self) -> u32 {
        self.total_tokens
    }

    pub fn cached_tokens(&self) -> u32 {
        self.prompt_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens)
            .unwrap_or_default()
    }
}

/// One generated text, as both a streamed chunk and a whole response carry it.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CompletionChoice {
    pub index: u32,
    #[serde(default)]
    pub text: String,
    /// `None` is absent, `Some(None)` is an explicit `null`, so both round-trip.
    #[serde(default, with = "serde_with::rust::double_option")]
    #[schemars(with = "Option<Option<CompletionLogprobs>>")]
    pub logprobs: Option<Option<CompletionLogprobs>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl CompletionChoice {
    /// Appends a later piece of the same choice: its text and log probabilities follow this
    /// one's, and its other fields (such as `finish_reason`) replace this one's.
    pub fn append(mut self, rhs: Self) -> Self {
        self.text.push_str(&rhs.text);
        self.logprobs = match (self.logprobs, rhs.logprobs) {
            (Some(Some(lhs)), Some(Some(rhs))) => Some(Some(lhs.append(rhs))),
            (lhs, None) => lhs,
            (Some(Some(lhs)), Some(None)) => Some(Some(lhs)),
            (_, rhs) => rhs,
        };
        self.additional_properties = self.additional_properties.merge(rhs.additional_properties);
        self
    }
}

/// The legacy completions log probabilities: one entry per sampled token in each list.
#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct CompletionLogprobs {
    pub tokens: Option<Vec<String>>,
    pub token_logprobs: Option<Vec<Option<f64>>>,
    pub top_logprobs: Option<Vec<Option<IndexMap<String, f64>>>>,
    pub text_offset: Option<Vec<u32>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl CompletionLogprobs {
    fn append(self, rhs: Self) -> Self {
        fn join<T>(lhs: Option<Vec<T>>, rhs: Option<Vec<T>>) -> Option<Vec<T>> {
            match (lhs, rhs) {
                (Some(mut lhs), Some(rhs)) => {
                    lhs.extend(rhs);
                    Some(lhs)
                }
                (lhs, rhs) => lhs.or(rhs),
            }
        }

        Self {
            tokens: join(self.tokens, rhs.tokens),
            token_logprobs: join(self.token_logprobs, rhs.token_logprobs),
            top_logprobs: join(self.top_logprobs, rhs.top_logprobs),
            text_offset: join(self.text_offset, rhs.text_offset),
            additional_properties: self.additional_properties.merge(rhs.additional_properties),
        }
    }
}
