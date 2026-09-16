use stefans_utils::literals::{False, True};

use crate::responses::models::api::request::{
    ResponsesRequestBody, common::CommonResponsesRequestBody,
    streaming::StreamingResponsesRequestBody,
};

#[derive(..ApiModel)]
pub struct NonStreamingResponsesRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<False>,
    #[serde(flatten)]
    pub common: CommonResponsesRequestBody,
}

impl From<NonStreamingResponsesRequestBody> for StreamingResponsesRequestBody {
    fn from(value: NonStreamingResponsesRequestBody) -> Self {
        Self {
            stream: True,
            common: value.common,
        }
    }
}

impl From<NonStreamingResponsesRequestBody> for ResponsesRequestBody {
    fn from(value: NonStreamingResponsesRequestBody) -> Self {
        Self::NonStreaming(value)
    }
}
