use reqwest::Client;
use std::{fmt::Display, time::Duration};
use stefans_utils::{prelude::AsClone, secret::Secret};
pub mod reasoning_remapping;

use crate::{
    apis::responses::models::lib::options::reasoning_remapping::ResponsesReasoningRemappingConfig,
    non_streaming_mode::NonStreamingMode, tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL,
    traits::max_tps::MaxTps,
};

pub struct ResponsesOptions<'a> {
    pub client: Option<Client>,
    pub bearer_token: Option<Secret<String>>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub max_tps_sample_interval: Option<Duration>,
    pub reasoning_remapping: Option<ResponsesReasoningRemappingConfig>,
    pub non_streaming_mode: NonStreamingMode,
}

impl Default for ResponsesOptions<'_> {
    fn default() -> Self {
        Self {
            client: None,
            bearer_token: None,
            max_tps: None,
            max_tps_sample_interval: Some(DEFAULT_MAX_TPS_SAMPLE_INTERVAL),
            reasoning_remapping: None,
            non_streaming_mode: NonStreamingMode::default(),
        }
    }
}

impl ResponsesOptions<'_> {
    pub fn with_client(mut self, client: impl AsClone<Client>) -> Self {
        self.client = Some(client.as_clone());
        self
    }

    pub fn with_default_client(mut self) -> Self {
        self.client = None;
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

    pub fn with_max_tps<'a>(self, max_tps: &'a dyn MaxTps) -> ResponsesOptions<'a> {
        ResponsesOptions {
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
        reasoning_remapping: impl Into<ResponsesReasoningRemappingConfig>,
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
