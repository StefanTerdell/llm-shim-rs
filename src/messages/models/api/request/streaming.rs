use stefans_utils::literals::True;

use crate::messages::models::api::request::{
    MessagesRequestBody, common::CommonMessagesRequestBody,
    non_streaming::NonStreamingMessagesRequestBody,
};

#[derive(..ApiModel)]
pub struct StreamingMessagesRequestBody {
    pub stream: True,
    #[serde(flatten)]
    pub common: CommonMessagesRequestBody,
}

impl From<StreamingMessagesRequestBody> for NonStreamingMessagesRequestBody {
    fn from(value: StreamingMessagesRequestBody) -> Self {
        Self {
            stream: None,
            common: value.common,
        }
    }
}

impl From<StreamingMessagesRequestBody> for MessagesRequestBody {
    fn from(value: StreamingMessagesRequestBody) -> Self {
        Self::Streaming(value)
    }
}
