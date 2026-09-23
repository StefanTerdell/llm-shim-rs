use crate::apis::chat_completion::models::api::{
    common::ChatCompletionUsage,
    response::{
        common::{ChatCompletionResponseMessage, CommonChatCompletionChoice},
        streaming::StreamingChatCompletionChoice,
    },
};

use crate::stats::StreamStats;
use indexmap::IndexMap;
use serde_json::Value;
use std::ops::Add;
use stefans_utils::literal_str;

pub struct NonStreamingChatCompletionResponse {
    pub body: NonStreamingChatCompletionResponseBody,
    pub stats: Option<StreamStats>,
    pub error: Option<Value>,
}

literal_str!(ChatCompletionObjectLabel = "chat.completion");

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingChatCompletionResponseBody {
    #[serde(default)]
    pub object: ChatCompletionObjectLabel,
    pub model: Option<String>,
    #[serde(default)]
    pub choices: Vec<NonStreamingChatCompletionChoice>,
    pub usage: Option<ChatCompletionUsage>,
    pub error: Option<Value>,
    #[serde(flatten)]
    pub additional_properties: IndexMap<String, Value>,
}

#[derive(..ApiModel)]
pub struct NonStreamingChatCompletionChoice {
    pub message: ChatCompletionResponseMessage,
    #[serde(flatten)]
    pub common: CommonChatCompletionChoice,
}

impl From<StreamingChatCompletionChoice> for NonStreamingChatCompletionChoice {
    fn from(value: StreamingChatCompletionChoice) -> Self {
        Self {
            message: value.delta,
            common: value.common,
        }
    }
}

impl Add for NonStreamingChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            message: self.message + rhs.message,
            common: self.common + rhs.common,
        }
    }
}

impl Add<StreamingChatCompletionChoice> for NonStreamingChatCompletionChoice {
    type Output = Self;

    fn add(self, rhs: StreamingChatCompletionChoice) -> Self::Output {
        self + Self::from(rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn round_trip(v: Value) -> Value {
        serde_json::to_value(
            serde_json::from_value::<NonStreamingChatCompletionResponseBody>(v).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn unknown_fields_and_missing_usage_round_trip() {
        let full = json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1,
            "model": "m",
            "system_fingerprint": "fp",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3}
        });
        assert_eq!(round_trip(full.clone()), full);

        let without_usage = json!({
            "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}}]
        });
        let parsed =
            serde_json::from_value::<NonStreamingChatCompletionResponseBody>(without_usage.clone())
                .unwrap();
        assert!(parsed.usage.is_none());
        assert_eq!(round_trip(without_usage.clone()), without_usage);
    }

    #[test]
    fn a_body_without_object_parses_and_serializes_the_label() {
        let body =
            json!({"choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}}]});
        let parsed =
            serde_json::from_value::<NonStreamingChatCompletionResponseBody>(body).unwrap();
        assert_eq!(
            serde_json::to_value(&parsed).unwrap()["object"],
            json!("chat.completion")
        );
    }

    #[test]
    fn error_field_is_parsed() {
        let body =
            json!({"object": "chat.completion", "choices": [], "error": {"message": "boom"}});
        let parsed =
            serde_json::from_value::<NonStreamingChatCompletionResponseBody>(body).unwrap();
        assert_eq!(parsed.error, Some(json!({"message": "boom"})));
    }
}
