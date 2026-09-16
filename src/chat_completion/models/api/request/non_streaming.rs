use stefans_utils::literals::{False, True};

use crate::chat_completion::models::api::request::{
    ChatCompletionRequestBody, common::CommonChatCompletionRequestBody,
    streaming::StreamingChatCompletionRequestBody,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingChatCompletionRequestBody {
    pub stream: Option<False>,
    #[serde(flatten)]
    pub common: CommonChatCompletionRequestBody,
}

impl From<NonStreamingChatCompletionRequestBody> for StreamingChatCompletionRequestBody {
    fn from(value: NonStreamingChatCompletionRequestBody) -> Self {
        Self {
            stream: True,
            stream_options: None,
            common: value.common,
        }
    }
}

impl From<NonStreamingChatCompletionRequestBody> for ChatCompletionRequestBody {
    fn from(value: NonStreamingChatCompletionRequestBody) -> Self {
        Self::NonStreaming(value)
    }
}
