use reqwest::Client;
use std::{fmt::Display, time::Duration};
use stefans_utils::{prelude::AsClone, secret::Secret};

use crate::{tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL, traits::max_tps::MaxTps};

pub struct EmbeddingsOptions<'a> {
    pub client: Option<Client>,
    pub bearer_token: Option<Secret<String>>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub max_tps_sample_interval: Option<Duration>,
}

impl Default for EmbeddingsOptions<'_> {
    fn default() -> Self {
        Self {
            client: None,
            bearer_token: None,
            max_tps: None,
            max_tps_sample_interval: Some(DEFAULT_MAX_TPS_SAMPLE_INTERVAL),
        }
    }
}

impl EmbeddingsOptions<'_> {
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

    pub fn with_max_tps<'a>(self, max_tps: &'a dyn MaxTps) -> EmbeddingsOptions<'a> {
        EmbeddingsOptions {
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
}
