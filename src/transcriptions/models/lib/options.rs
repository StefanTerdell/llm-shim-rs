use reqwest::Client;
use std::fmt::Display;
use stefans_utils::{prelude::AsClone, secret::Secret};

use crate::traits::max_tps::MaxTps;

#[derive(Default)]
pub struct TranscriptionsOptions<'a> {
    pub client: Option<Client>,
    pub bearer_token: Option<Secret<String>>,
    pub tps_throttler: Option<&'a dyn MaxTps>,
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

    pub fn with_bearer_token<T: Display>(mut self, bearer_token: impl Into<Secret<T>>) -> Self {
        self.bearer_token = Some(bearer_token.into().expose_to_string().into());
        self
    }

    pub fn without_bearer_token(mut self) -> Self {
        self.bearer_token = None;
        self
    }

    pub fn with_tps_throttler<'a>(
        self,
        tps_throttler: &'a dyn MaxTps,
    ) -> TranscriptionsOptions<'a> {
        TranscriptionsOptions {
            tps_throttler: Some(tps_throttler),
            ..self
        }
    }

    pub fn without_tps_throttler(mut self) -> Self {
        self.tps_throttler = None;
        self
    }
}
