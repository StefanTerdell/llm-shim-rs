use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct NonStreamingTranscriptionsResponse {
    pub body: TranscriptionsResponseBody,
    pub stats: StreamStats,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptionsResponseBody {
    Json(TranscriptionsJsonBody),
    Text(String),
}

impl TranscriptionsResponseBody {
    pub fn text(&self) -> &str {
        match self {
            Self::Json(body) => &body.text,
            Self::Text(text) => text,
        }
    }

    pub fn usage(&self) -> Option<&TranscriptionsUsage> {
        match self {
            Self::Json(body) => body.usage.as_ref(),
            Self::Text(_) => None,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct TranscriptionsJsonBody {
    pub text: String,
    pub usage: Option<TranscriptionsUsage>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct TranscriptionsUsage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}
