use reqwest::Client;
use std::{fmt::Display, time::Duration};
use stefans_utils::{prelude::AsClone, secret::Secret};
pub mod reasoning_remapping;

use crate::{
    apis::chat_completion::models::lib::options::reasoning_remapping::ChatCompletionReasoningRemappingConfig,
    non_streaming_mode::NonStreamingMode, tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL,
    traits::max_tps::MaxTps,
};

pub struct ChatCompletionOptions<'a> {
    pub client: Option<Client>,
    pub bearer_token: Option<Secret<String>>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub max_tps_sample_interval: Option<Duration>,
    pub reasoning_remapping: Option<ChatCompletionReasoningRemappingConfig>,
    pub output_token_counting: OutputTokenCounting,
    pub non_streaming_mode: NonStreamingMode,
}

impl Default for ChatCompletionOptions<'_> {
    fn default() -> Self {
        Self {
            client: None,
            bearer_token: None,
            max_tps: None,
            max_tps_sample_interval: Some(DEFAULT_MAX_TPS_SAMPLE_INTERVAL),
            reasoning_remapping: None,
            output_token_counting: OutputTokenCounting::default(),
            non_streaming_mode: NonStreamingMode::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OutputTokenCounting {
    #[default]
    Logprobs,
    Estimate,
}

impl ChatCompletionOptions<'_> {
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

    pub fn with_max_tps<'a>(self, max_tps: &'a dyn MaxTps) -> ChatCompletionOptions<'a> {
        ChatCompletionOptions {
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
        reasoning_remapping: impl Into<ChatCompletionReasoningRemappingConfig>,
    ) -> Self {
        self.reasoning_remapping = Some(reasoning_remapping.into());
        self
    }

    pub fn without_reasoning_remapping(mut self) -> Self {
        self.reasoning_remapping = None;
        self
    }

    pub fn with_output_token_counting(
        mut self,
        output_token_counting: OutputTokenCounting,
    ) -> Self {
        self.output_token_counting = output_token_counting;
        self
    }

    pub fn with_non_streaming_mode(mut self, mode: NonStreamingMode) -> Self {
        self.non_streaming_mode = mode;
        self
    }
}
