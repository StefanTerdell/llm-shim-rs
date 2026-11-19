mod support;

use llm_shim::prelude::*;
use serde_json::{Value, json};
use support::*;
use tokio_stream::StreamExt;

fn text(index: u32, text: &str) -> Value {
    json!({"id": "cmpl-1", "object": "text_completion", "created": 7, "model": "m", "choices": [{"index": index, "text": text, "logprobs": null, "finish_reason": null}]})
}

fn usage(prompt: u32, completion: u32, cached: u32) -> Value {
    json!({"id": "cmpl-1", "object": "text_completion", "choices": [], "usage": {"prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion, "prompt_tokens_details": {"cached_tokens": cached}}})
}

fn body(extra: Value) -> Value {
    let mut body = json!({"model": "test-model", "prompt": "Once upon a time"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    body
}

async fn collect(server: &MockSse, extra: Value) -> Vec<StreamingCompletionEvent> {
    let mut request = body(json!({"stream": true}));
    request
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    let request: StreamingCompletionRequestBody = serde_json::from_value(request).unwrap();

    let mut stream = streaming_completion(&server.completions_url, request, None)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

#[tokio::test]
async fn streams_text_and_reports_exact_usage_with_cached_tokens() {
    let server = MockSse::start(Script::sse([
        data(text(0, "there")),
        data(text(0, " lived")),
        data(usage(4, 2, 3)),
        done(),
    ]))
    .await;

    let events = collect(&server, json!({})).await;

    let texts: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            StreamingCompletionEvent::Chunk { chunk } => chunk
                .choices
                .as_ref()
                .and_then(|choices| choices.first())
                .map(|choice| choice.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["there", " lived"]);

    let StreamingCompletionEvent::Done { stats } = events.last().unwrap() else {
        panic!("the stream should end with Done");
    };
    assert_eq!(stats.input_tokens, 4);
    assert_eq!(stats.output_tokens, 2);
    assert_eq!(stats.cached_input_tokens, 3);
    assert!(!stats.output_tokens_is_estimate);

    let sent = server.single_request();
    assert_eq!(sent["stream_options"]["include_usage"], json!(true));
    assert_eq!(sent["prompt"], json!("Once upon a time"));
}

#[tokio::test]
async fn usage_reaches_the_caller_only_when_asked_for() {
    for (asked, expected) in [(false, false), (true, true)] {
        let server = MockSse::start(Script::sse([
            data(text(0, "a")),
            data(usage(1, 1, 0)),
            done(),
        ]))
        .await;

        let events = collect(&server, json!({"stream_options": {"include_usage": asked}})).await;

        let carries_usage = events.iter().any(|event| {
            matches!(event, StreamingCompletionEvent::Chunk { chunk } if chunk.usage.is_some())
        });
        assert_eq!(carries_usage, expected, "include_usage: {asked}");
    }
}

#[tokio::test]
async fn estimates_output_without_usage() {
    let server = MockSse::start(Script::sse([data(text(0, "abcdefg")), done()])).await;

    let events = collect(&server, json!({})).await;

    let StreamingCompletionEvent::Done { stats } = events.last().unwrap() else {
        panic!("the stream should end with Done");
    };
    assert!(stats.output_tokens > 0);
    assert!(stats.output_tokens_is_estimate);
}

#[tokio::test]
async fn an_error_frame_ends_the_stream() {
    let server = MockSse::start(Script::sse([
        data(text(0, "a")),
        data(json!({"error": {"message": "boom", "type": "server_error"}})),
        data(text(0, "never")),
    ]))
    .await;

    let events = collect(&server, json!({})).await;

    let StreamingCompletionEvent::ChunkError { stats, chunk } = events.last().unwrap() else {
        panic!("the stream should end with ChunkError");
    };
    assert!(stats.error.as_deref().unwrap().contains("boom"));
    assert!(chunk.error.is_some());
    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn virtual_non_streaming_collects_choices_and_keeps_the_stream_metadata() {
    let server = MockSse::start(Script::sse([
        data(text(0, "there")),
        data(text(1, "B")),
        data(json!({"id": "cmpl-1", "object": "text_completion", "created": 7, "model": "m", "choices": [{"index": 0, "text": " lived", "logprobs": null, "finish_reason": "stop"}]})),
        data(usage(4, 3, 0)),
        done(),
    ]))
    .await;

    let request: NonStreamingCompletionRequestBody =
        serde_json::from_value(body(json!({}))).unwrap();
    let response = virtual_non_streaming_completion(&server.completions_url, request, None)
        .await
        .unwrap();

    assert_eq!(
        serde_json::to_value(&response.body).unwrap(),
        json!({
            "object": "text_completion",
            "model": "test-model",
            "choices": [
                {"index": 0, "text": "there lived", "logprobs": null, "finish_reason": "stop"},
                {"index": 1, "text": "B", "logprobs": null, "finish_reason": null}
            ],
            "usage": {"prompt_tokens": 4, "completion_tokens": 3, "total_tokens": 7},
            "id": "cmpl-1",
            "created": 7
        })
    );
    assert_eq!(server.single_request()["stream"], json!(true));
}

#[tokio::test]
async fn virtual_non_streaming_appends_logprobs() {
    let chunk = |token: &str, offset: u32| json!({"choices": [{"index": 0, "text": token, "logprobs": {"tokens": [token], "token_logprobs": [-0.5], "top_logprobs": null, "text_offset": [offset]}}]});
    let server = MockSse::start(Script::sse([
        data(chunk("a", 0)),
        data(chunk("b", 1)),
        done(),
    ]))
    .await;

    let request: NonStreamingCompletionRequestBody =
        serde_json::from_value(body(json!({"logprobs": 0}))).unwrap();
    let response = virtual_non_streaming_completion(&server.completions_url, request, None)
        .await
        .unwrap();

    let logprobs = response.body.choices[0].logprobs.clone().flatten().unwrap();
    assert_eq!(
        logprobs.tokens,
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(logprobs.text_offset, Some(vec![0, 1]));
}

#[tokio::test]
async fn real_non_streaming_reads_the_body_and_its_usage() {
    let server = MockSse::start(Script::json(json!({
        "id": "cmpl-9",
        "object": "text_completion",
        "model": "m",
        "choices": [{"index": 0, "text": "hi", "logprobs": null, "finish_reason": "length"}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 1, "total_tokens": 6, "prompt_tokens_details": {"cached_tokens": 2}}
    })))
    .await;

    let request: NonStreamingCompletionRequestBody =
        serde_json::from_value(body(json!({}))).unwrap();
    let response = non_streaming_completion(&server.completions_url, request, None)
        .await
        .unwrap();

    let stats = response.stats.unwrap();
    assert_eq!(stats.input_tokens, 5);
    assert_eq!(stats.cached_input_tokens, 2);
    assert_eq!(response.body.choices[0].text, "hi");
}

#[tokio::test]
async fn completion_dispatches_on_stream() {
    let server = MockSse::start(Script::sse([data(text(0, "x")), done()])).await;

    let request: CompletionRequestBody = serde_json::from_value(body(json!({}))).unwrap();
    let response = completion(&server.completions_url, request, None)
        .await
        .unwrap();

    assert!(matches!(response, CompletionResponse::NonStreaming(_)));
}
