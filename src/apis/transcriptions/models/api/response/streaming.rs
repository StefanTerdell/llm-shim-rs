use indexmap::IndexMap;
use serde_json::Value;

use crate::apis::transcriptions::models::api::response::non_streaming::{
    TranscriptionsUsage, TranscriptionsUsageType,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
#[serde(tag = "type")]
pub enum TranscriptionsStreamEvent {
    #[serde(rename = "transcript.text.delta")]
    Delta {
        delta: String,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "transcript.text.done")]
    Done {
        text: String,
        usage: Option<TranscriptionsUsage>,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(rename = "error")]
    Error {
        error: Value,
        #[serde(flatten)]
        additional_properties: IndexMap<String, Value>,
    },
    #[serde(untagged)]
    Other(Value),
}

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum CompatTranscriptionsChunk {
    ChatCompletion(ChatCompletionTranscriptionsChunk),
    Text(TextTranscriptionsChunk),
}

impl CompatTranscriptionsChunk {
    pub fn usage(&self) -> Option<TranscriptionsUsage> {
        match self {
            Self::ChatCompletion(chunk) => chunk.usage.clone().map(TranscriptionsUsage::from),
            Self::Text(chunk) => chunk.usage_or_duration(),
        }
    }

    pub fn id_mut(&mut self) -> &mut Option<String> {
        match self {
            Self::ChatCompletion(chunk) => &mut chunk.id,
            Self::Text(chunk) => &mut chunk.id,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionTranscriptionsChunk {
    pub id: Option<String>,
    pub choices: Vec<ChatCompletionTranscriptionsChoice>,
    pub usage: Option<ChatCompletionTranscriptionsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct ChatCompletionTranscriptionsChoice {
    #[serde(default)]
    pub delta: ChatCompletionTranscriptionsDelta,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct ChatCompletionTranscriptionsDelta {
    pub content: Option<String>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct TextTranscriptionsChunk {
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub id: Option<String>,
    pub text: String,
    pub usage: Option<TranscriptionsUsage>,
    pub duration: Option<f64>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl TextTranscriptionsChunk {
    pub fn usage_or_duration(&self) -> Option<TranscriptionsUsage> {
        self.usage
            .clone()
            .or_else(|| self.duration.map(TranscriptionsUsage::duration))
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct ChatCompletionTranscriptionsUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl From<ChatCompletionTranscriptionsUsage> for TranscriptionsUsage {
    fn from(usage: ChatCompletionTranscriptionsUsage) -> Self {
        Self {
            usage_type: Some(TranscriptionsUsageType::Tokens),
            input_tokens: Some(usage.prompt_tokens),
            output_tokens: Some(usage.completion_tokens),
            total_tokens: usage.total_tokens,
            seconds: None,
            additional_properties: usage.additional_properties,
        }
    }
}
