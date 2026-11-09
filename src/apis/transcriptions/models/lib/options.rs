use reqwest::Client;
use std::time::Duration;
use stefans_utils::prelude::AsClone;

use crate::credentials::Credentials;

use crate::{tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL, traits::max_tps::MaxTps};

pub struct TranscriptionsOptions<'a> {
    pub client: Option<Client>,
    pub credentials: Option<Credentials>,
    pub max_tps: Option<&'a dyn MaxTps>,
    pub max_tps_sample_interval: Option<Duration>,
}

impl Default for TranscriptionsOptions<'_> {
    fn default() -> Self {
        Self {
            client: None,
            credentials: None,
            max_tps: None,
            max_tps_sample_interval: Some(DEFAULT_MAX_TPS_SAMPLE_INTERVAL),
        }
    }
}

impl TranscriptionsOptions<'_> {
    pub fn with_client(mut self, client: impl AsClone<Client>) -> Self {
        self.client = Some(client.as_clone());
        self
    }

    pub fn with_default_client(mut self) -> Self {
        self.client = None;
        self
    }

    pub fn with_credentials(mut self, credentials: impl Into<Credentials>) -> Self {
        self.credentials = Some(credentials.into());
        self
    }

    pub fn without_credentials(mut self) -> Self {
        self.credentials = None;
        self
    }

    pub fn with_max_tps<'a>(self, max_tps: &'a dyn MaxTps) -> TranscriptionsOptions<'a> {
        TranscriptionsOptions {
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
