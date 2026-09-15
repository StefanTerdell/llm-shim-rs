use reqwest::Client;
use std::fmt::Display;
use stefans_utils::{prelude::AsClone, secret::Secret};
pub mod reasoning_content_remapping;

use crate::{
    chat_completion::models::lib::options::reasoning_content_remapping::ReasoningContentRemappingConfig,
    traits::max_tps::MaxTps,
};

#[derive(Default)]
pub struct ChatCompletionOptions<'a> {
    pub client: Option<Client>,
    pub bearer_token: Option<Secret<String>>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub reasoning_content_remapping: Option<ReasoningContentRemappingConfig>,
    pub output_token_counting: OutputTokenCounting,
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

    pub fn with_max_tps<'a>(self, tps_throttler: &'a dyn MaxTps) -> ChatCompletionOptions<'a> {
        ChatCompletionOptions {
            max_tps: Some(tps_throttler),
            ..self
        }
    }

    pub fn without_max_tps(mut self) -> Self {
        self.max_tps = None;
        self
    }

    pub fn with_remap_reasoning(
        mut self,
        remap_reasoning: impl Into<ReasoningContentRemappingConfig>,
    ) -> Self {
        self.reasoning_content_remapping = Some(remap_reasoning.into());
        self
    }

    pub fn without_remap_reasoning(mut self) -> Self {
        self.reasoning_content_remapping = None;
        self
    }

    pub fn with_output_token_counting(
        mut self,
        output_token_counting: OutputTokenCounting,
    ) -> Self {
        self.output_token_counting = output_token_counting;
        self
    }
}
