use std::fmt::Display;

use indexmap::IndexMap;
use serde_json::Value;
use stefans_utils::literals::{False, True};

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum CompletionRequestBody {
    NonStreaming(NonStreamingCompletionRequestBody),
    Streaming(StreamingCompletionRequestBody),
}

impl CompletionRequestBody {
    pub fn with_model(self, model: Option<impl Display>) -> Self {
        match self {
            Self::NonStreaming(body) => Self::NonStreaming(body.with_model(model)),
            Self::Streaming(body) => Self::Streaming(body.with_model(model)),
        }
    }
}

impl From<CompletionRequestBody> for NonStreamingCompletionRequestBody {
    fn from(value: CompletionRequestBody) -> Self {
        match value {
            CompletionRequestBody::NonStreaming(body) => body,
            CompletionRequestBody::Streaming(body) => body.into(),
        }
    }
}

impl From<CompletionRequestBody> for StreamingCompletionRequestBody {
    fn from(value: CompletionRequestBody) -> Self {
        match value {
            CompletionRequestBody::Streaming(body) => body,
            CompletionRequestBody::NonStreaming(body) => body.into(),
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct CommonCompletionRequestBody {
    pub model: Option<String>,
    pub prompt: CompletionPrompt,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl CommonCompletionRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.model = model.map(|t| t.to_string());
        self
    }
}

/// What a legacy completion continues: text, or token ids, alone or in a batch.
#[derive(..ApiModel)]
#[serde(untagged)]
pub enum CompletionPrompt {
    Text(String),
    Texts(Vec<String>),
    Tokens(Vec<u32>),
    TokenBatches(Vec<Vec<u32>>),
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingCompletionRequestBody {
    pub stream: Option<False>,
    #[serde(flatten)]
    pub common: CommonCompletionRequestBody,
}

impl NonStreamingCompletionRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.common = self.common.with_model(model);
        self
    }
}

impl From<NonStreamingCompletionRequestBody> for StreamingCompletionRequestBody {
    fn from(value: NonStreamingCompletionRequestBody) -> Self {
        Self {
            stream: True,
            stream_options: None,
            common: value.common,
        }
    }
}

impl From<NonStreamingCompletionRequestBody> for CompletionRequestBody {
    fn from(value: NonStreamingCompletionRequestBody) -> Self {
        Self::NonStreaming(value)
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingCompletionRequestBody {
    pub stream: True,
    pub stream_options: Option<StreamingCompletionRequestBodyStreamOptions>,
    #[serde(flatten)]
    pub common: CommonCompletionRequestBody,
}

impl StreamingCompletionRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.common = self.common.with_model(model);
        self
    }
}

impl From<StreamingCompletionRequestBody> for NonStreamingCompletionRequestBody {
    fn from(value: StreamingCompletionRequestBody) -> Self {
        Self {
            stream: None,
            common: value.common,
        }
    }
}

impl From<StreamingCompletionRequestBody> for CompletionRequestBody {
    fn from(value: StreamingCompletionRequestBody) -> Self {
        Self::Streaming(value)
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct StreamingCompletionRequestBodyStreamOptions {
    pub include_usage: Option<bool>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
