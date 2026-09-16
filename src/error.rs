pub use eventsource_stream::EventStreamError;
pub use reqwest::Error as ReqwestError;
pub use reqwest::StatusCode;
pub use serde_json::Error as SerdeJsonError;
pub use url::ParseError as UrlParseError;

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
}
