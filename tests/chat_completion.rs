mod support;

use llm_shim::apis::chat_completion::{
    models::{
        api::request::streaming::StreamingChatCompletionRequestBody,
        lib::streaming::response::StreamingChatCompletionEvent,
    },
    streaming::streaming_chat_completion,
};
use serde_json::{Value, json};
use support::*;
use tokio_stream::StreamExt;

fn usage(prompt: u32, completion: u32) -> Value {
    json!({"choices": [], "usage": {"prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion}})
}

async fn collect(
    server: &MockSse,
    body: StreamingChatCompletionRequestBody,
) -> Vec<StreamingChatCompletionEvent> {
    let mut stream = streaming_chat_completion(&server.url, body, None)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

#[tokio::test]
async fn streams_chunks_then_done_with_usage_from_server() {
    let server = MockSse::start(Script::sse([
        data(delta(0, "Hel")),
        data(delta(0, "lo")),
        data(usage(7, 2)),
        done(),
    ]))
    .await;

    let events = collect(&server, streaming_request(json!({}))).await;

    assert_eq!(events.len(), 4);
    let contents: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            StreamingChatCompletionEvent::Chunk { chunk } => chunk
                .choices
                .as_ref()
                .and_then(|c| c.first())
                .and_then(|c| c.delta.content.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(contents, ["Hel", "lo"]);

    let StreamingChatCompletionEvent::Done { stats } = events.last().unwrap() else {
        panic!("last event should be Done");
    };
    assert!(!stats.input_tokens_is_estimate);
    assert!(!stats.output_tokens_is_estimate);
    assert_eq!(stats.input_tokens, 7);
    assert_eq!(stats.output_tokens, 2);
    assert_eq!(stats.chunks.len(), 3);
    assert!(stats.error.is_none());
}

#[tokio::test]
async fn forces_logprobs_and_include_usage_in_sent_request() {
    let server = MockSse::start(Script::sse([done()])).await;

    collect(&server, streaming_request(json!({}))).await;

    let sent = server.single_request();
    assert_eq!(sent["logprobs"], json!(true));
    assert_eq!(sent["stream_options"]["include_usage"], json!(true));
    assert_eq!(sent["stream"], json!(true));
    assert_eq!(sent["model"], json!("test-model"));
}

fn delta_with_logprobs(index: u32, content: &str, tokens: usize) -> Value {
    let logprobs: Vec<Value> = (0..tokens)
        .map(|i| json!({"token": format!("t{i}"), "logprob": -0.1}))
        .collect();
    json!({"choices": [{
        "index": index,
        "delta": {"role": "assistant", "content": content},
        "logprobs": {"content": logprobs},
    }]})
}

fn chunks(events: &[StreamingChatCompletionEvent]) -> Vec<&llm_shim::apis::chat_completion::models::api::response::streaming::StreamingChatCompletionChunk>{
    events
        .iter()
        .filter_map(|e| match e {
            StreamingChatCompletionEvent::Chunk { chunk } => Some(chunk),
            StreamingChatCompletionEvent::ChunkError { chunk, .. } => Some(chunk),
            _ => None,
        })
        .collect()
}

fn done_stats(events: &[StreamingChatCompletionEvent]) -> &llm_shim::stats::StreamStats {
    match events.last().unwrap() {
        StreamingChatCompletionEvent::Done { stats } => stats,
        _ => panic!("last event should be Done"),
    }
}

#[tokio::test]
async fn strips_logprobs_and_usage_when_caller_did_not_request_them() {
    let server = MockSse::start(Script::sse([
        data(delta_with_logprobs(0, "Hello", 3)),
        data(usage(7, 3)),
        done(),
    ]))
    .await;

    let events = collect(&server, streaming_request(json!({}))).await;

    let chunks = chunks(&events);
    assert_eq!(chunks.len(), 2);
    assert!(
        chunks[0].choices.as_ref().unwrap()[0]
            .common
            .logprobs
            .is_none()
    );
    assert!(chunks[1].usage.is_none());
}

#[tokio::test]
async fn keeps_logprobs_and_usage_when_caller_requested_them() {
    let server = MockSse::start(Script::sse([
        data(delta_with_logprobs(0, "Hello", 3)),
        data(usage(7, 3)),
        done(),
    ]))
    .await;

    let events = collect(
        &server,
        streaming_request(json!({"logprobs": true, "stream_options": {"include_usage": true}})),
    )
    .await;

    let chunks = chunks(&events);
    assert_eq!(chunks.len(), 2);
    let logprobs = chunks[0].choices.as_ref().unwrap()[0]
        .common
        .logprobs
        .as_ref()
        .unwrap();
    assert_eq!(logprobs["content"].len(), 3);
    assert_eq!(chunks[1].usage.as_ref().unwrap().completion_tokens(), 3);
}

#[tokio::test]
async fn counts_output_tokens_exactly_from_logprobs_before_usage_arrives() {
    let server = MockSse::start(Script::sse([
        data(delta_with_logprobs(0, "Hel", 2)),
        data(delta_with_logprobs(0, "lo", 3)),
        done(),
    ]))
    .await;

    let events = collect(&server, streaming_request(json!({}))).await;

    let stats = done_stats(&events);
    assert!(!stats.output_tokens_is_estimate);
    assert_eq!(stats.output_tokens, 5);
    assert!(stats.input_tokens_is_estimate);
    assert!(stats.input_tokens > 0);
    let per_chunk: Vec<_> = stats.chunks.iter().map(|c| c.tokens).collect();
    assert_eq!(per_chunk, [2, 3]);
}

#[tokio::test]
async fn estimates_output_tokens_when_server_sends_no_logprobs() {
    let server = MockSse::start(Script::sse([
        data(delta(0, "Hello there, how are you?")),
        done(),
    ]))
    .await;

    let events = collect(&server, streaming_request(json!({}))).await;

    let stats = done_stats(&events);
    assert!(stats.output_tokens_is_estimate);
    assert!(stats.chunks[0].tokens > 0);
}

#[tokio::test]
async fn error_chunk_yields_chunk_error_and_ends_stream() {
    let server = MockSse::start(Script::sse([
        data(delta(0, "Hel")),
        data(json!({"error": {"message": "boom", "code": 500}})),
        data(delta(0, "lo")),
        done(),
    ]))
    .await;

    let events = collect(&server, streaming_request(json!({}))).await;

    assert_eq!(events.len(), 2);
    let StreamingChatCompletionEvent::ChunkError { stats, chunk } = &events[1] else {
        panic!("second event should be ChunkError");
    };
    assert!(chunk.error.is_some());
    assert!(stats.error.as_deref().unwrap().contains("boom"));
}

mod non_streaming {
    use super::*;
    use llm_shim::apis::chat_completion::{
        models::{
            api::request::non_streaming::NonStreamingChatCompletionRequestBody,
            lib::options::{
                ChatCompletionOptions, reasoning_remapping::ChatCompletionReasoningPosition,
            },
        },
        non_streaming::non_streaming_chat_completion,
    };

    fn request() -> NonStreamingChatCompletionRequestBody {
        serde_json::from_value(json!({
            "model": "test-model",
            "messages": [{"role": "user", "content": "hello"}],
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn folds_interleaved_choices_by_index() {
        let server = MockSse::start(Script::sse([
            data(delta(0, "Hel")),
            data(delta(1, "Wor")),
            data(delta(0, "lo")),
            data(delta(1, "ld")),
            data(usage(7, 4)),
            done(),
        ]))
        .await;

        let response = non_streaming_chat_completion(&server.url, request(), None)
            .await
            .unwrap();

        let mut choices = response.body.choices;
        choices.sort_by_key(|c| c.common.index);
        let contents: Vec<_> = choices
            .iter()
            .map(|c| c.message.content.clone().unwrap())
            .collect();
        assert_eq!(contents, ["Hello", "World"]);
        assert_eq!(
            choices[0].message.common.additional_properties["role"],
            json!("assistant")
        );
        assert_eq!(response.body.usage.prompt_tokens(), 7);
        assert_eq!(response.body.usage.completion_tokens(), 4);
        assert_eq!(response.body.usage.total_tokens(), 11);
        assert!(response.stats.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.body.model.as_deref(), Some("test-model"));

        let sent = server.single_request();
        assert_eq!(sent["stream"], json!(true));
    }

    #[tokio::test]
    async fn falls_back_to_counted_tokens_when_server_sends_no_usage() {
        let server = MockSse::start(Script::sse([
            data(delta_with_logprobs(0, "Hel", 2)),
            data(delta_with_logprobs(0, "lo", 3)),
            done(),
        ]))
        .await;

        let response = non_streaming_chat_completion(&server.url, request(), None)
            .await
            .unwrap();

        assert_eq!(response.body.usage.completion_tokens(), 5);
        assert!(response.body.usage.prompt_tokens() > 0);
    }

    #[tokio::test]
    async fn surfaces_error_chunk() {
        let server = MockSse::start(Script::sse([
            data(delta(0, "Hel")),
            data(json!({"error": {"message": "boom"}})),
        ]))
        .await;

        let response = non_streaming_chat_completion(&server.url, request(), None)
            .await
            .unwrap();

        assert_eq!(response.error.unwrap()["message"], json!("boom"));
        assert_eq!(
            response.body.choices[0].message.content.as_deref(),
            Some("Hel")
        );
        assert_eq!(
            response.stats.unwrap().error.as_deref(),
            Some(r#"{"message":"boom"}"#)
        );
    }

    #[tokio::test]
    async fn remaps_tagged_reasoning_into_reasoning_content_across_chunks() {
        let server = MockSse::start(Script::sse([
            data(delta(0, "<thi")),
            data(delta(0, "nk>plan")),
            data(delta(0, "ning</think>ans")),
            data(delta(0, "wer")),
            done(),
        ]))
        .await;

        let options = ChatCompletionOptions::default().with_reasoning_remapping((
            ChatCompletionReasoningPosition::content_unchecked("<think>", "</think>"),
            ChatCompletionReasoningPosition::ReasoningContent,
        ));

        let response = non_streaming_chat_completion(&server.url, request(), options)
            .await
            .unwrap();

        let message = &response.body.choices[0].message;
        assert_eq!(message.content.as_deref(), Some("answer"));
        assert_eq!(
            message.common.reasoning_content.as_deref(),
            Some("planning")
        );
    }
}

mod throttling {
    use super::*;
    use async_trait::async_trait;
    use llm_shim::{
        apis::chat_completion::models::lib::options::ChatCompletionOptions, traits::max_tps::MaxTps,
    };
    use std::time::{Duration, Instant};

    struct FixedTps(f32);

    #[async_trait]
    impl MaxTps for FixedTps {
        async fn get(&self) -> Option<f32> {
            Some(self.0)
        }
    }

    #[tokio::test]
    async fn slows_stream_down_to_max_tps() {
        const CHUNKS: u32 = 5;
        const TOKENS_PER_CHUNK: u32 = 4;
        const MAX_TPS: f32 = 100.0;

        let frames = (0..CHUNKS)
            .map(|_| data(delta_with_logprobs(0, "x", TOKENS_PER_CHUNK as usize)))
            .chain([done()]);
        let server = MockSse::start(Script::sse(frames)).await;

        let throttler = FixedTps(MAX_TPS);
        let options = ChatCompletionOptions::default().with_max_tps(&throttler);

        let started = Instant::now();
        let mut stream =
            streaming_chat_completion(&server.url, streaming_request(json!({})), options)
                .await
                .unwrap();
        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }
        let elapsed = started.elapsed();

        let total_tokens = CHUNKS * TOKENS_PER_CHUNK;
        let floor = Duration::from_secs_f32(total_tokens as f32 / MAX_TPS);
        assert!(
            elapsed >= floor - Duration::from_millis(20),
            "elapsed {elapsed:?} should be at least ~{floor:?}"
        );

        let stats = done_stats(&events);
        assert_eq!(stats.output_tokens, total_tokens);
        assert!(stats.tps_avg().unwrap() <= MAX_TPS + 1.0);
        assert!(
            stats
                .chunks
                .iter()
                .any(|c| c.tps_correction_duration.is_some())
        );
    }

    #[tokio::test]
    async fn does_not_delay_a_stream_already_under_the_limit() {
        let frames = (0..3)
            .map(|_| {
                after(
                    Duration::from_millis(30),
                    data(delta_with_logprobs(0, "x", 1)),
                )
            })
            .chain([done()]);
        let server = MockSse::start(Script::sse(frames)).await;

        let throttler = FixedTps(1000.0);
        let options = ChatCompletionOptions::default().with_max_tps(&throttler);

        let mut stream =
            streaming_chat_completion(&server.url, streaming_request(json!({})), options)
                .await
                .unwrap();
        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        let stats = done_stats(&events);
        assert!(
            stats
                .chunks
                .iter()
                .all(|c| c.tps_correction_duration.is_none())
        );
    }
}
