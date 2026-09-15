use stefans_utils::literals::{False, True};

use crate::messages::models::api::request::{
    MessagesRequestBody, common::CommonMessagesRequestBody, streaming::StreamingMessagesRequestBody,
};

#[derive(..ApiModel)]
pub struct NonStreamingMessagesRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<False>,
    #[serde(flatten)]
    pub common: CommonMessagesRequestBody,
}

impl From<NonStreamingMessagesRequestBody> for StreamingMessagesRequestBody {
    fn from(value: NonStreamingMessagesRequestBody) -> Self {
        Self {
            stream: True,
            common: value.common,
        }
    }
}

impl From<NonStreamingMessagesRequestBody> for MessagesRequestBody {
    fn from(value: NonStreamingMessagesRequestBody) -> Self {
        Self::NonStreaming(value)
    }
}
