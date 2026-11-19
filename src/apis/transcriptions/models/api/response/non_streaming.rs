use indexmap::IndexMap;
use serde_json::Value;

use crate::stats::StreamStats;

pub struct NonStreamingTranscriptionsResponse {
    pub body: TranscriptionsResponseBody,
    pub usage: Option<TranscriptionsUsage>,
    pub stats: StreamStats,
}

#[allow(clippy::large_enum_variant)]
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
    pub id: Option<String>,
    pub text: String,
    pub usage: Option<TranscriptionsUsage>,
    pub duration: Option<f64>,
    pub language: Option<String>,
    pub segments: Option<Vec<TranscriptionsSegment>>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct TranscriptionsSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl TranscriptionsJsonBody {
    pub fn usage_or_duration(&self) -> Option<TranscriptionsUsage> {
        self.usage
            .clone()
            .or_else(|| self.duration.map(TranscriptionsUsage::duration))
    }
}

#[serde_with::skip_serializing_none]
#[derive(..ApiModel, Default)]
pub struct TranscriptionsUsage {
    #[serde(rename = "type")]
    pub usage_type: Option<TranscriptionsUsageType>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub seconds: Option<f64>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

impl TranscriptionsUsage {
    pub fn duration(seconds: f64) -> Self {
        Self {
            usage_type: Some(TranscriptionsUsageType::Duration),
            seconds: Some(seconds),
            ..Default::default()
        }
    }
}

#[derive(..ApiModel, Copy, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptionsUsageType {
    Tokens,
    Duration,
}
