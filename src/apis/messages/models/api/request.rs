pub mod common;
pub mod non_streaming;
pub mod streaming;

use std::fmt::Display;

use crate::apis::messages::models::api::request::{
    non_streaming::NonStreamingMessagesRequestBody, streaming::StreamingMessagesRequestBody,
};

#[derive(..ApiModel)]
#[serde(untagged)]
pub enum MessagesRequestBody {
    NonStreaming(NonStreamingMessagesRequestBody),
    Streaming(StreamingMessagesRequestBody),
}

impl MessagesRequestBody {
    pub fn with_model(self, model: Option<impl Display>) -> Self {
        match self {
            Self::NonStreaming(body) => Self::NonStreaming(body.with_model(model)),
            Self::Streaming(body) => Self::Streaming(body.with_model(model)),
        }
    }
}

impl From<MessagesRequestBody> for NonStreamingMessagesRequestBody {
    fn from(value: MessagesRequestBody) -> Self {
        match value {
            MessagesRequestBody::NonStreaming(x) => x,
            MessagesRequestBody::Streaming(x) => x.into(),
        }
    }
}

impl From<MessagesRequestBody> for StreamingMessagesRequestBody {
    fn from(value: MessagesRequestBody) -> Self {
        match value {
            MessagesRequestBody::Streaming(x) => x,
            MessagesRequestBody::NonStreaming(x) => x.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stream_flag_selects_the_variant() {
        let body = json!({"model": "m", "max_tokens": 10, "messages": [{"role": "user", "content": "hi"}]});
        assert!(matches!(
            serde_json::from_value::<MessagesRequestBody>(body.clone()).unwrap(),
            MessagesRequestBody::NonStreaming(_)
        ));

        let mut streaming = body;
        streaming["stream"] = json!(true);
        assert!(matches!(
            serde_json::from_value::<MessagesRequestBody>(streaming).unwrap(),
            MessagesRequestBody::Streaming(_)
        ));
    }
}
