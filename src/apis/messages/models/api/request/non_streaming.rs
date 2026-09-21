use std::fmt::Display;

use stefans_utils::literals::{False, True};

use crate::apis::messages::models::api::request::{
    MessagesRequestBody, common::CommonMessagesRequestBody, streaming::StreamingMessagesRequestBody,
};

#[serde_with::skip_serializing_none]
#[derive(..ApiModel)]
pub struct NonStreamingMessagesRequestBody {
    pub stream: Option<False>,
    #[serde(flatten)]
    pub common: CommonMessagesRequestBody,
}

impl NonStreamingMessagesRequestBody {
    pub fn with_model(mut self, model: Option<impl Display>) -> Self {
        self.common = self.common.with_model(model);
        self
    }
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
