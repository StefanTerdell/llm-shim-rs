use reqwest::Client;
use std::{fmt::Display, time::Duration};
use stefans_utils::{prelude::AsClone, secret::Secret};
pub mod reasoning_remapping;

use crate::{
    apis::messages::models::lib::options::reasoning_remapping::MessagesReasoningRemappingConfig,
    non_streaming_mode::NonStreamingMode, tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL,
    traits::max_tps::MaxTps,
};

pub struct MessagesOptions<'a> {
    pub client: Option<Client>,
    pub api_key: Option<Secret<String>>,
    pub bearer_token: Option<Secret<String>>,
    pub anthropic_version: Option<String>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub max_tps_sample_interval: Option<Duration>,
    pub reasoning_remapping: Option<MessagesReasoningRemappingConfig>,
    pub non_streaming_mode: NonStreamingMode,
}

impl Default for MessagesOptions<'_> {
    fn default() -> Self {
        Self {
            client: None,
            api_key: None,
            bearer_token: None,
            anthropic_version: None,
            max_tps: None,
            max_tps_sample_interval: Some(DEFAULT_MAX_TPS_SAMPLE_INTERVAL),
            reasoning_remapping: None,
            non_streaming_mode: NonStreamingMode::default(),
        }
    }
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

    pub fn with_max_tps<'a>(self, max_tps: &'a dyn MaxTps) -> MessagesOptions<'a> {
        MessagesOptions {
            max_tps: Some(max_tps),
            ..self
        }
    }

    pub fn without_max_tps(mut self) -> Self {
        self.max_tps = None;
        self
    }

    pub fn with_max_tps_sample_interval(mut self, interval: Duration) -> Self {
        self.max_tps_sample_interval = Some(interval);
        self
    }

    pub fn without_max_tps_sample_interval(mut self) -> Self {
        self.max_tps_sample_interval = None;
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

    pub fn with_non_streaming_mode(mut self, mode: NonStreamingMode) -> Self {
        self.non_streaming_mode = mode;
        self
    }
}
