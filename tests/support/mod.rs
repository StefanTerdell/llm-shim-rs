#![allow(dead_code)] // shared between test binaries; each uses a subset

//! A tiny in-process SSE server for driving the client under test.
//!
//! Frames are raw strings so tests can send malformed JSON, split frames, or
//! non-2xx bodies. Each frame may be preceded by a delay so throttling tests can
//! control chunk timing.

use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::{HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::Response,
    routing::post,
};
use serde_json::Value;
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle, time::sleep};

#[derive(Clone, Debug)]
pub struct Frame {
    pub delay: Option<Duration>,
    pub raw: String,
}

/// `data: <json>\n\n`
pub fn data(json: Value) -> Frame {
    raw(format!("data: {json}\n\n"))
}

/// A frame with arbitrary wire content (no framing added).
pub fn raw(raw: impl Into<String>) -> Frame {
    Frame {
        delay: None,
        raw: raw.into(),
    }
}

/// `data: [DONE]\n\n`
pub fn done() -> Frame {
    raw("data: [DONE]\n\n")
}

pub fn after(delay: Duration, mut frame: Frame) -> Frame {
    frame.delay = Some(delay);
    frame
}

#[derive(Clone, Debug)]
pub struct Script {
    pub status: StatusCode,
    pub content_type: &'static str,
    pub frames: Vec<Frame>,
}

impl Script {
    /// A well-formed `text/event-stream` response.
    pub fn sse(frames: impl IntoIterator<Item = Frame>) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "text/event-stream",
            frames: frames.into_iter().collect(),
        }
    }

    /// A non-streaming error response with a JSON body.
    pub fn error(status: StatusCode, body: Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            frames: vec![raw(body.to_string())],
        }
    }
}

#[derive(Clone)]
struct AppState {
    script: Arc<Script>,
    requests: Arc<Mutex<Vec<Value>>>,
}

pub struct MockSse {
    pub url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    _server: JoinHandle<()>,
}

impl MockSse {
    pub async fn start(script: Script) -> Self {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = AppState {
            script: Arc::new(script),
            requests: requests.clone(),
        };

        let app = Router::new()
            .route("/v1/chat/completions", post(handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            url: format!("http://{addr}/v1/chat/completions"),
            requests,
            _server: server,
        }
    }

    /// Request bodies received so far, parsed as JSON.
    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    pub fn single_request(&self) -> Value {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests.into_iter().next().unwrap()
    }
}

impl Drop for MockSse {
    fn drop(&mut self) {
        self._server.abort();
    }
}

async fn handler(State(state): State<AppState>, body: Bytes) -> Response {
    let json = serde_json::from_slice::<Value>(&body).expect("request body must be JSON");
    state.requests.lock().unwrap().push(json);

    let frames = state.script.frames.clone();
    let stream = async_stream::stream! {
        for frame in frames {
            if let Some(delay) = frame.delay {
                sleep(delay).await;
            }
            yield Ok::<Bytes, Infallible>(Bytes::from(frame.raw));
        }
    };

    Response::builder()
        .status(state.script.status)
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static(state.script.content_type),
        )
        .body(Body::from_stream(stream))
        .unwrap()
}

/// A minimal streaming chat completion request, with `extra` merged into the body.
pub fn streaming_request(
    extra: Value,
) -> llm_stream_map::chat_completion::models::api::request::streaming::StreamingChatCompletionRequestBody{
    let mut body = serde_json::json!({
        "model": "test-model",
        "stream": true,
        "messages": [{"role": "user", "content": "hello"}],
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

/// A content delta chunk for one choice.
pub fn delta(index: u32, content: &str) -> Value {
    serde_json::json!({"choices": [{"index": index, "delta": {"role": "assistant", "content": content}}]})
}
