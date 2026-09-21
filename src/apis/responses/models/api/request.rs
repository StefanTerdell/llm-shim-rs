pub mod common;
pub mod non_streaming;
pub mod streaming;

use std::fmt::Display;

use crate::apis::responses::models::api::request::{
    non_streaming::NonStreamingResponsesRequestBody, streaming::StreamingResponsesRequestBody,
};

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum ResponsesRequestBody {
    NonStreaming(NonStreamingResponsesRequestBody),
    Streaming(StreamingResponsesRequestBody),
}

impl ResponsesRequestBody {
    pub fn with_model(self, model: Option<impl Display>) -> Self {
        match self {
            Self::NonStreaming(body) => Self::NonStreaming(body.with_model(model)),
            Self::Streaming(body) => Self::Streaming(body.with_model(model)),
        }
    }
}

impl From<ResponsesRequestBody> for NonStreamingResponsesRequestBody {
    fn from(value: ResponsesRequestBody) -> Self {
        match value {
            ResponsesRequestBody::NonStreaming(x) => x,
            ResponsesRequestBody::Streaming(x) => x.into(),
        }
    }
}

impl From<ResponsesRequestBody> for StreamingResponsesRequestBody {
    fn from(value: ResponsesRequestBody) -> Self {
        match value {
            ResponsesRequestBody::Streaming(x) => x,
            ResponsesRequestBody::NonStreaming(x) => x.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stream_flag_selects_the_variant_and_input_accepts_text_or_items() {
        let text = json!({"model": "m", "input": "hi"});
        assert!(matches!(
            serde_json::from_value::<ResponsesRequestBody>(text).unwrap(),
            ResponsesRequestBody::NonStreaming(_)
        ));

        let items = json!({"model": "m", "stream": true, "input": [{"role": "user", "content": [{"type": "input_text", "text": "hi"}]}]});
        assert!(matches!(
            serde_json::from_value::<ResponsesRequestBody>(items).unwrap(),
            ResponsesRequestBody::Streaming(_)
        ));
    }
}
