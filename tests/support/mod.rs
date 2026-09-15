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
    http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE},
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
    requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
}

pub struct MockSse {
    pub url: String,
    pub messages_url: String,
    requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
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
            .route("/v1/messages", post(handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            url: format!("http://{addr}/v1/chat/completions"),
            messages_url: format!("http://{addr}/v1/messages"),
            requests,
            _server: server,
        }
    }

    /// Request bodies received so far, parsed as JSON.
    pub fn requests(&self) -> Vec<Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|(_, body)| body.clone())
            .collect()
    }

    pub fn single_request(&self) -> Value {
        let requests = self.requests();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests.into_iter().next().unwrap()
    }

    pub fn single_request_headers(&self) -> HeaderMap {
        let requests = self.requests.lock().unwrap();
        assert_eq!(requests.len(), 1, "expected exactly one request");
        requests[0].0.clone()
    }
}

impl Drop for MockSse {
    fn drop(&mut self) {
        self._server.abort();
    }
}

async fn handler(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let json = serde_json::from_slice::<Value>(&body).expect("request body must be JSON");
    state.requests.lock().unwrap().push((headers, json));

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

pub mod anthropic {
    use super::*;

    pub fn request(
        extra: Value,
    ) -> llm_stream_map::messages::models::api::request::streaming::StreamingMessagesRequestBody
    {
        let mut body = serde_json::json!({
            "model": "test-model",
            "max_tokens": 64,
            "stream": true,
            "messages": [{"role": "user", "content": "hello"}],
        });
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().cloned().unwrap_or_default());
        serde_json::from_value(body).unwrap()
    }

    pub fn non_streaming_request() -> llm_stream_map::messages::models::api::request::non_streaming::NonStreamingMessagesRequestBody{
        serde_json::from_value(serde_json::json!({
            "model": "test-model",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hello"}],
        }))
        .unwrap()
    }

    /// `event: <type>\ndata: <json>\n\n`, as Anthropic sends it.
    pub fn event(json: Value) -> Frame {
        let event_type = json["type"]
            .as_str()
            .expect("event needs a type")
            .to_owned();
        raw(format!("event: {event_type}\ndata: {json}\n\n"))
    }

    pub fn message_start(input_tokens: u32) -> Value {
        serde_json::json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "model": "test-model", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": input_tokens, "output_tokens": 1}}})
    }

    pub fn text_start(index: u32) -> Value {
        serde_json::json!({"type": "content_block_start", "index": index, "content_block": {"type": "text", "text": ""}})
    }

    pub fn text_delta(index: u32, text: &str) -> Value {
        serde_json::json!({"type": "content_block_delta", "index": index, "delta": {"type": "text_delta", "text": text}})
    }

    pub fn thinking_start(index: u32) -> Value {
        serde_json::json!({"type": "content_block_start", "index": index, "content_block": {"type": "thinking", "thinking": "", "signature": ""}})
    }

    pub fn thinking_delta(index: u32, thinking: &str) -> Value {
        serde_json::json!({"type": "content_block_delta", "index": index, "delta": {"type": "thinking_delta", "thinking": thinking}})
    }

    pub fn signature_delta(index: u32, signature: &str) -> Value {
        serde_json::json!({"type": "content_block_delta", "index": index, "delta": {"type": "signature_delta", "signature": signature}})
    }

    pub fn tool_use_start(index: u32, id: &str, name: &str) -> Value {
        serde_json::json!({"type": "content_block_start", "index": index, "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}})
    }

    pub fn input_json_delta(index: u32, partial_json: &str) -> Value {
        serde_json::json!({"type": "content_block_delta", "index": index, "delta": {"type": "input_json_delta", "partial_json": partial_json}})
    }

    pub fn block_stop(index: u32) -> Value {
        serde_json::json!({"type": "content_block_stop", "index": index})
    }

    pub fn message_delta(stop_reason: &str, output_tokens: u32) -> Value {
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": stop_reason, "stop_sequence": null}, "usage": {"output_tokens": output_tokens}})
    }

    pub fn message_stop() -> Value {
        serde_json::json!({"type": "message_stop"})
    }

    pub fn ping() -> Value {
        serde_json::json!({"type": "ping"})
    }

    /// A complete plain text reply as Anthropic would stream it.
    pub fn simple_text_script(
        text_chunks: &[&str],
        input_tokens: u32,
        output_tokens: u32,
    ) -> Script {
        let mut frames = vec![event(message_start(input_tokens)), event(text_start(0))];
        frames.extend(text_chunks.iter().map(|t| event(text_delta(0, t))));
        frames.extend([
            event(block_stop(0)),
            event(message_delta("end_turn", output_tokens)),
            event(message_stop()),
        ]);
        Script::sse(frames)
    }
}
