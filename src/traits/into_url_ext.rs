use reqwest::IntoUrl;

use crate::error::Error;

pub trait IntoUrlExt {
    fn with_default_chat_completions_path(self) -> Result<impl IntoUrl, Error>;
    fn with_default_messages_path(self) -> Result<impl IntoUrl, Error>;
    fn with_default_responses_path(self) -> Result<impl IntoUrl, Error>;
}

impl<T: IntoUrl> IntoUrlExt for T {
    fn with_default_chat_completions_path(self) -> Result<impl IntoUrl, Error> {
        Ok(self.into_url()?.join("/v1/chat/completions")?)
    }

    fn with_default_messages_path(self) -> Result<impl IntoUrl, Error> {
        Ok(self.into_url()?.join("/v1/messages")?)
    }

    fn with_default_responses_path(self) -> Result<impl IntoUrl, Error> {
        Ok(self.into_url()?.join("/v1/responses")?)
    }
}
