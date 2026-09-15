use crate::error::Error;
use eventsource_stream::Eventsource;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use serde_json::error::Category;
use tokio_stream::{Stream, StreamExt};

pub(crate) use eventsource_stream::Event;

pub(crate) async fn send(
    request: RequestBuilder,
) -> Result<impl Stream<Item = Result<Event, Error>> + Send + 'static, Error> {
    Ok(request
        .send()
        .await?
        .error_for_status()?
        .bytes_stream()
        .eventsource()
        .map(|result| result.map_err(Error::from)))
}

#[derive(Default)]
pub(crate) struct JsonFrameBuffer {
    partial: Option<String>,
}

impl JsonFrameBuffer {
    pub fn push<T: DeserializeOwned>(&mut self, mut data: String) -> Result<Option<T>, Error> {
        if let Some(previous) = self.partial.take() {
            data = format!("{previous}{data}");
        }

        match serde_json::from_str::<T>(&data) {
            Ok(value) => Ok(Some(value)),
            Err(e) if e.classify() == Category::Eof => {
                self.partial = Some(data);
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }
}
