#![allow(dead_code)] // shared between test binaries; each uses a subset

//! A tiny in-process SSE server for driving the client under test.
//!
//! Frames are raw strings so tests can send malformed JSON, split frames, or
//! non-2xx bodies. Each frame may be preceded by a delay so throttling tests can
//! control chunk timing.

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{FromRequest, Multipart, Request, State},
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

    /// A plain 200 text/plain response.
    pub fn text(body: impl Into<String>) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "text/plain; charset=utf-8",
            frames: vec![raw(body.into())],
        }
    }

    /// A plain 200 JSON response.
    pub fn json(body: Value) -> Self {
        Self {
            status: StatusCode::OK,
            content_type: "application/json",
            frames: vec![raw(body.to_string())],
        }
    }

    /// Delay the whole response body by `delay`.
    pub fn delayed(mut self, delay: Duration) -> Self {
        if let Some(first) = self.frames.first_mut() {
            first.delay = Some(delay);
        }
        self
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
    pub responses_url: String,
    pub embeddings_url: String,
    pub rerank_url: String,
    pub transcriptions_url: String,
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
            .route("/v1/responses", post(handler))
            .route("/v1/embeddings", post(handler))
            .route("/v1/rerank", post(handler))
            .route("/v1/audio/transcriptions", post(handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        Self {
            url: format!("http://{addr}/v1/chat/completions"),
            messages_url: format!("http://{addr}/v1/messages"),
            responses_url: format!("http://{addr}/v1/responses"),
            embeddings_url: format!("http://{addr}/v1/embeddings"),
            rerank_url: format!("http://{addr}/v1/rerank"),
            transcriptions_url: format!("http://{addr}/v1/audio/transcriptions"),
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

async fn handler(State(state): State<AppState>, request: Request) -> Response {
    let headers = request.headers().clone();
    let is_multipart = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("multipart/form-data"));

    let json = if is_multipart {
        let mut multipart = Multipart::from_request(request, &()).await.unwrap();
        let mut object = serde_json::Map::new();
        while let Some(field) = multipart.next_field().await.unwrap() {
            let name = field.name().unwrap().to_owned();
            if let Some(filename) = field.file_name().map(str::to_owned) {
                let content_type = field.content_type().map(str::to_owned);
                let bytes = field.bytes().await.unwrap();
                object.insert(
                    name,
                    serde_json::json!({
                        "filename": filename,
                        "content_type": content_type,
                        "size": bytes.len(),
                        "content": String::from_utf8_lossy(&bytes),
                    }),
                );
            } else {
                let text = field.text().await.unwrap();
                match name.strip_suffix("[]") {
                    Some(array_name) => {
                        object
                            .entry(array_name.to_owned())
                            .or_insert_with(|| Value::Array(vec![]))
                            .as_array_mut()
                            .unwrap()
                            .push(Value::String(text));
                    }
                    None => {
                        object.insert(name, Value::String(text));
                    }
                }
            }
        }
        Value::Object(object)
    } else {
        let body = axum::body::to_bytes(request.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice::<Value>(&body).expect("request body must be JSON")
    };

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
) -> llm_shim::apis::chat_completion::models::api::request::streaming::StreamingChatCompletionRequestBody{
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
    ) -> llm_shim::apis::messages::models::api::request::streaming::StreamingMessagesRequestBody
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

    pub fn non_streaming_request() -> llm_shim::apis::messages::models::api::request::non_streaming::NonStreamingMessagesRequestBody{
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

pub mod openai_responses {
    use super::*;

    pub fn request(
        extra: Value,
    ) -> llm_shim::apis::responses::models::api::request::streaming::StreamingResponsesRequestBody
    {
        let mut body = serde_json::json!({"model": "test-model", "stream": true, "input": "hello"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().cloned().unwrap_or_default());
        serde_json::from_value(body).unwrap()
    }

    pub fn non_streaming_request() -> llm_shim::apis::responses::models::api::request::non_streaming::NonStreamingResponsesRequestBody{
        serde_json::from_value(serde_json::json!({"model": "test-model", "input": "hello"}))
            .unwrap()
    }

    /// `event: <type>\ndata: <json>\n\n`, as OpenAI sends it.
    pub fn event(json: Value) -> Frame {
        let event_type = json["type"]
            .as_str()
            .expect("event needs a type")
            .to_owned();
        raw(format!("event: {event_type}\ndata: {json}\n\n"))
    }

    pub fn response_object(status: &str, output: Value, usage: Option<(u32, u32)>) -> Value {
        let mut r = serde_json::json!({"id": "resp_1", "object": "response", "created_at": 1, "status": status, "model": "test-model", "output": output});
        if let Some((input, output)) = usage {
            r["usage"] = serde_json::json!({"input_tokens": input, "output_tokens": output, "total_tokens": input + output, "output_tokens_details": {"reasoning_tokens": 0}});
        }
        r
    }

    pub fn created() -> Value {
        serde_json::json!({"type": "response.created", "sequence_number": 0, "response": response_object("in_progress", serde_json::json!([]), None)})
    }

    pub fn completed(output: Value, input_tokens: u32, output_tokens: u32) -> Value {
        serde_json::json!({"type": "response.completed", "sequence_number": 99, "response": response_object("completed", output, Some((input_tokens, output_tokens)))})
    }

    pub fn message_item(id: &str, text: &str, status: &str) -> Value {
        let content = if text.is_empty() {
            serde_json::json!([])
        } else {
            serde_json::json!([{"type": "output_text", "text": text, "annotations": []}])
        };
        serde_json::json!({"type": "message", "id": id, "status": status, "role": "assistant", "content": content})
    }

    pub fn reasoning_item(id: &str, summaries: &[&str]) -> Value {
        let summary: Vec<Value> = summaries
            .iter()
            .map(|s| serde_json::json!({"type": "summary_text", "text": s}))
            .collect();
        serde_json::json!({"type": "reasoning", "id": id, "summary": summary})
    }

    pub fn item_added(output_index: u32, item: Value) -> Value {
        serde_json::json!({"type": "response.output_item.added", "output_index": output_index, "item": item, "sequence_number": 1})
    }

    pub fn item_done(output_index: u32, item: Value) -> Value {
        serde_json::json!({"type": "response.output_item.done", "output_index": output_index, "item": item, "sequence_number": 1})
    }

    pub fn part_added(item_id: &str, output_index: u32, content_index: u32) -> Value {
        serde_json::json!({"type": "response.content_part.added", "item_id": item_id, "output_index": output_index, "content_index": content_index, "part": {"type": "output_text", "text": "", "annotations": []}, "sequence_number": 1})
    }

    pub fn part_done(item_id: &str, output_index: u32, content_index: u32, text: &str) -> Value {
        serde_json::json!({"type": "response.content_part.done", "item_id": item_id, "output_index": output_index, "content_index": content_index, "part": {"type": "output_text", "text": text, "annotations": []}, "sequence_number": 1})
    }

    pub fn text_delta(item_id: &str, output_index: u32, delta: &str) -> Value {
        serde_json::json!({"type": "response.output_text.delta", "item_id": item_id, "output_index": output_index, "content_index": 0, "delta": delta, "logprobs": [], "sequence_number": 1})
    }

    pub fn text_done(item_id: &str, output_index: u32, text: &str) -> Value {
        serde_json::json!({"type": "response.output_text.done", "item_id": item_id, "output_index": output_index, "content_index": 0, "text": text, "logprobs": [], "sequence_number": 1})
    }

    pub fn summary_part_added(item_id: &str, output_index: u32, summary_index: u32) -> Value {
        serde_json::json!({"type": "response.reasoning_summary_part.added", "item_id": item_id, "output_index": output_index, "summary_index": summary_index, "part": {"type": "summary_text", "text": ""}, "sequence_number": 1})
    }

    pub fn summary_part_done(
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        text: &str,
    ) -> Value {
        serde_json::json!({"type": "response.reasoning_summary_part.done", "item_id": item_id, "output_index": output_index, "summary_index": summary_index, "part": {"type": "summary_text", "text": text}, "sequence_number": 1})
    }

    pub fn summary_delta(
        item_id: &str,
        output_index: u32,
        summary_index: u32,
        delta: &str,
    ) -> Value {
        serde_json::json!({"type": "response.reasoning_summary_text.delta", "item_id": item_id, "output_index": output_index, "summary_index": summary_index, "delta": delta, "sequence_number": 1})
    }

    pub fn summary_done(item_id: &str, output_index: u32, summary_index: u32, text: &str) -> Value {
        serde_json::json!({"type": "response.reasoning_summary_text.done", "item_id": item_id, "output_index": output_index, "summary_index": summary_index, "text": text, "sequence_number": 1})
    }

    pub fn reasoning_text_delta(item_id: &str, output_index: u32, delta: &str) -> Value {
        serde_json::json!({"type": "response.reasoning_text.delta", "item_id": item_id, "output_index": output_index, "content_index": 0, "delta": delta, "sequence_number": 1})
    }

    /// A complete plain text reply as OpenAI would stream it.
    pub fn simple_text_script(chunks: &[&str], input_tokens: u32, output_tokens: u32) -> Script {
        let text: String = chunks.concat();
        let mut frames = vec![
            event(created()),
            event(item_added(0, message_item("msg_1", "", "in_progress"))),
            event(part_added("msg_1", 0, 0)),
        ];
        frames.extend(chunks.iter().map(|c| event(text_delta("msg_1", 0, c))));
        frames.extend([
            event(text_done("msg_1", 0, &text)),
            event(part_done("msg_1", 0, 0, &text)),
            event(item_done(0, message_item("msg_1", &text, "completed"))),
            event(completed(
                serde_json::json!([message_item("msg_1", &text, "completed")]),
                input_tokens,
                output_tokens,
            )),
        ]);
        Script::sse(frames)
    }
}
