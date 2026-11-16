pub use eventsource_stream::EventStreamError;
pub use reqwest::Error as ReqwestError;
pub use reqwest::StatusCode;
pub use serde_json::Error as SerdeJsonError;
pub use url::ParseError as UrlParseError;

use reqwest::{RequestBuilder, Response};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] ReqwestError),
    #[error(transparent)]
    SerdeJson(#[from] SerdeJsonError),
    #[error(transparent)]
    EventStream(#[from] EventStreamError<ReqwestError>),
    #[error(transparent)]
    UrlParse(#[from] UrlParseError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("upstream responded with {status}")]
    UpstreamStatus { status: StatusCode, body: String },
    #[error("{0}")]
    Translation(String),
}

pub(crate) async fn send_checked(request: RequestBuilder) -> Result<Response, Error> {
    let response = request.send().await?;
    let status = response.status();

    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await.unwrap_or_default();

    Err(Error::UpstreamStatus { status, body })
}
