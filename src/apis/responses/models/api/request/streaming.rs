use stefans_utils::literals::True;

use crate::apis::responses::models::api::request::{
    ResponsesRequestBody, common::CommonResponsesRequestBody,
    non_streaming::NonStreamingResponsesRequestBody,
};

#[derive(..ApiModel)]
pub struct StreamingResponsesRequestBody {
    pub stream: True,
    #[serde(flatten)]
    pub common: CommonResponsesRequestBody,
}

impl From<StreamingResponsesRequestBody> for NonStreamingResponsesRequestBody {
    fn from(value: StreamingResponsesRequestBody) -> Self {
        Self {
            stream: None,
            common: value.common,
        }
    }
}

impl From<StreamingResponsesRequestBody> for ResponsesRequestBody {
    fn from(value: StreamingResponsesRequestBody) -> Self {
        Self::Streaming(value)
    }
}
