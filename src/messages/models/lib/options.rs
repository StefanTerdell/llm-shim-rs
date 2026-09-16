use reqwest::Client;
use std::fmt::Display;
use stefans_utils::{prelude::AsClone, secret::Secret};
pub mod reasoning_remapping;

use crate::{
    messages::models::lib::options::reasoning_remapping::MessagesReasoningRemappingConfig,
    traits::max_tps::MaxTps,
};

#[derive(Default)]
pub struct MessagesOptions<'a> {
    pub client: Option<Client>,
    pub api_key: Option<Secret<String>>,
    pub bearer_token: Option<Secret<String>>,
    pub anthropic_version: Option<String>,
    pub tps_throttler: Option<&'a dyn MaxTps>,
    pub reasoning_remapping: Option<MessagesReasoningRemappingConfig>,
}

impl MessagesOptions<'_> {
    pub fn with_client(mut self, client: impl AsClone<Client>) -> Self {
        self.client = Some(client.as_clone());
        self
    }

    pub fn with_default_client(mut self) -> Self {
        self.client = None;
        self
    }

    pub fn with_api_key<T: Display>(mut self, api_key: impl Into<Secret<T>>) -> Self {
        self.api_key = Some(api_key.into().expose_to_string().into());
        self
    }

    pub fn without_api_key(mut self) -> Self {
        self.api_key = None;
        self
    }

    pub fn with_bearer_token<T: Display>(mut self, bearer_token: impl Into<Secret<T>>) -> Self {
        self.bearer_token = Some(bearer_token.into().expose_to_string().into());
        self
    }

    pub fn without_bearer_token(mut self) -> Self {
        self.bearer_token = None;
        self
    }

    pub fn with_anthropic_version(mut self, version: impl Into<String>) -> Self {
        self.anthropic_version = Some(version.into());
        self
    }

    pub fn with_tps_throttler<'a>(self, tps_throttler: &'a dyn MaxTps) -> MessagesOptions<'a> {
        MessagesOptions {
            tps_throttler: Some(tps_throttler),
            ..self
        }
    }

    pub fn without_tps_throttler(mut self) -> Self {
        self.tps_throttler = None;
        self
    }

    pub fn with_reasoning_remapping(
        mut self,
        reasoning_remapping: impl Into<MessagesReasoningRemappingConfig>,
    ) -> Self {
        self.reasoning_remapping = Some(reasoning_remapping.into());
        self
    }

    pub fn without_reasoning_remapping(mut self) -> Self {
        self.reasoning_remapping = None;
        self
    }
}
