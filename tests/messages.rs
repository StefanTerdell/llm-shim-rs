mod support;

use llm_shim::{
    apis::messages::{
        models::{
            api::{
                common::{ContentBlock, ContentBlockDelta},
                request::streaming::StreamingMessagesRequestBody,
                response::streaming::MessagesStreamEvent,
            },
            lib::{options::MessagesOptions, streaming::response::StreamingMessagesEvent},
        },
        non_streaming::non_streaming_messages,
        streaming::streaming_messages,
    },
    error::Error,
    stats::StreamStats,
};
use serde_json::json;
use support::{anthropic::*, *};
use tokio_stream::StreamExt;

async fn collect(
    server: &MockSse,
    body: StreamingMessagesRequestBody,
) -> Vec<StreamingMessagesEvent> {
    collect_with(server, body, None).await
}

async fn collect_with<'a>(
    server: &MockSse,
    body: StreamingMessagesRequestBody,
    options: impl Into<Option<MessagesOptions<'a>>>,
) -> Vec<StreamingMessagesEvent> {
    let mut stream = streaming_messages(&server.messages_url, body, options)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

fn raw_events(events: &[StreamingMessagesEvent]) -> Vec<&MessagesStreamEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            StreamingMessagesEvent::Event { event }
            | StreamingMessagesEvent::EventError { event, .. } => Some(event),
            _ => None,
        })
        .collect()
}

fn done_stats(events: &[StreamingMessagesEvent]) -> &StreamStats {
    match events.last().unwrap() {
        StreamingMessagesEvent::Done { stats } => stats,
        _ => panic!("last event should be Done"),
    }
}

fn texts(events: &[&MessagesStreamEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            MessagesStreamEvent::ContentBlockDelta {
                delta: ContentBlockDelta::TextDelta { text, .. },
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn passes_events_through_and_ends_with_done() {
    let server = MockSse::start(simple_text_script(&["Hel", "lo"], 25, 12)).await;

    let events = collect(&server, request(json!({}))).await;

    let raw = raw_events(&events);
    assert_eq!(raw.len(), 7);
    assert!(matches!(raw[0], MessagesStreamEvent::MessageStart { .. }));
    assert!(matches!(raw[6], MessagesStreamEvent::MessageStop { .. }));
    assert_eq!(texts(&raw), "Hello");
    assert_eq!(events.len(), 8);

    let stats = done_stats(&events);
    assert!(!stats.input_tokens_is_estimate);
    assert_eq!(stats.input_tokens, 25);
    assert!(!stats.output_tokens_is_estimate);
    assert_eq!(stats.output_tokens, 12);
    assert!(stats.error.is_none());
}

#[tokio::test]
async fn estimates_output_tokens_until_message_delta_arrives() {
    let server = MockSse::start(Script::sse([
        event(message_start(5)),
        event(text_start(0)),
        event(text_delta(0, "Hello there, how are you doing today?")),
        event(block_stop(0)),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    let stats = done_stats(&events);
    assert!(stats.output_tokens_is_estimate);
    assert!(stats.output_tokens > 0);
    assert_eq!(stats.chunks.len(), 2);
    assert!(stats.chunks[1].tokens > 0);
}

#[tokio::test]
async fn sends_api_key_and_version_headers_and_forces_stream() {
    let server = MockSse::start(simple_text_script(&["x"], 1, 1)).await;

    let options = MessagesOptions::default().with_api_key("sk-test");
    collect_with(&server, request(json!({})), options).await;

    let headers = server.single_request_headers();
    assert_eq!(headers["x-api-key"], "sk-test");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert_eq!(server.single_request()["stream"], json!(true));
}

#[tokio::test]
async fn bearer_token_and_custom_version_are_honoured() {
    let server = MockSse::start(simple_text_script(&["x"], 1, 1)).await;

    let options = MessagesOptions::default()
        .with_bearer_token("tok")
        .with_anthropic_version("2024-01-01");
    collect_with(&server, request(json!({})), options).await;

    let headers = server.single_request_headers();
    assert_eq!(headers["authorization"], "Bearer tok");
    assert_eq!(headers["anthropic-version"], "2024-01-01");
    assert!(headers.get("x-api-key").is_none());
}

#[tokio::test]
async fn error_event_yields_event_error_and_ends_stream() {
    let server = MockSse::start(Script::sse([
        event(message_start(5)),
        event(json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}})),
        event(text_start(0)),
        event(message_stop()),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    assert_eq!(events.len(), 2);
    let StreamingMessagesEvent::EventError { stats, event } = &events[1] else {
        panic!("second event should be EventError");
    };
    assert!(matches!(event, MessagesStreamEvent::Error { .. }));
    assert!(stats.error.as_deref().unwrap().contains("Overloaded"));
}

#[tokio::test]
async fn unknown_events_and_pings_pass_through() {
    let server = MockSse::start(Script::sse([
        event(message_start(5)),
        event(ping()),
        event(json!({"type": "future_event", "x": 1})),
        event(message_stop()),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    let raw = raw_events(&events);
    assert!(matches!(raw[1], MessagesStreamEvent::Other(_)));
    assert!(matches!(raw[2], MessagesStreamEvent::Other(_)));
    assert!(matches!(
        events.last().unwrap(),
        StreamingMessagesEvent::Done { .. }
    ));
}

#[tokio::test]
async fn data_only_frames_without_event_lines_are_accepted() {
    let server = MockSse::start(Script::sse([
        data(message_start(5)),
        data(text_start(0)),
        data(text_delta(0, "hi")),
        data(block_stop(0)),
        data(message_delta("end_turn", 1)),
        data(message_stop()),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    assert_eq!(texts(&raw_events(&events)), "hi");
    assert_eq!(done_stats(&events).output_tokens, 1);
}

#[tokio::test]
async fn non_2xx_surfaces_status_and_body() {
    let server = MockSse::start(Script::error(
        reqwest::StatusCode::UNAUTHORIZED,
        json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}),
    ))
    .await;

    match streaming_messages(&server.messages_url, request(json!({})), None).await {
        Err(Error::Reqwest(error)) => {
            assert_eq!(error.status(), Some(reqwest::StatusCode::UNAUTHORIZED));
        }
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("expected an error"),
    }
}

mod non_streaming {
    use super::*;

    #[tokio::test]
    async fn folds_text_thinking_and_tool_use_blocks() {
        let server = MockSse::start(Script::sse([
            event(message_start(30)),
            event(thinking_start(0)),
            event(thinking_delta(0, "Let me ")),
            event(thinking_delta(0, "think")),
            event(signature_delta(0, "sig123")),
            event(block_stop(0)),
            event(text_start(1)),
            event(text_delta(1, "Checking ")),
            event(text_delta(1, "weather")),
            event(block_stop(1)),
            event(tool_use_start(2, "toolu_1", "get_weather")),
            event(input_json_delta(2, "{\"city\": ")),
            event(input_json_delta(2, "\"Paris\"}")),
            event(block_stop(2)),
            event(message_delta("tool_use", 40)),
            event(message_stop()),
        ]))
        .await;

        let response = non_streaming_messages(&server.messages_url, non_streaming_request(), None)
            .await
            .unwrap();

        let body = response.body;
        assert_eq!(body.additional_properties["id"], json!("msg_1"));
        assert_eq!(body.additional_properties["role"], json!("assistant"));
        assert_eq!(body.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(body.usage.input_tokens, Some(30));
        assert_eq!(body.usage.output_tokens, Some(40));
        assert_eq!(body.content.len(), 3);
        assert_eq!(
            body.content[0],
            ContentBlock::thinking("Let me think", "sig123")
        );
        assert_eq!(body.content[1], ContentBlock::text("Checking weather"));
        let ContentBlock::ToolUse {
            name,
            input,
            additional_properties,
        } = &body.content[2]
        else {
            panic!("third block should be tool_use");
        };
        assert_eq!(additional_properties["id"], json!("toolu_1"));
        assert_eq!(name, "get_weather");
        assert_eq!(*input, json!({"city": "Paris"}));
        assert!(response.stats.is_some());
        assert!(response.error.is_none());

        assert_eq!(server.single_request()["stream"], json!(true));
    }

    #[tokio::test]
    async fn fills_usage_from_stats_when_server_omits_it() {
        let server = MockSse::start(Script::sse([
            event(json!({"type": "message_start", "message": {"id": "m", "role": "assistant", "content": [], "model": "x"}})),
            event(text_start(0)),
            event(text_delta(0, "Some words here")),
            event(block_stop(0)),
            event(message_stop()),
        ]))
        .await;

        let response = non_streaming_messages(&server.messages_url, non_streaming_request(), None)
            .await
            .unwrap();

        assert!(response.body.usage.input_tokens.unwrap() > 0);
        assert!(response.body.usage.output_tokens.unwrap() > 0);
        assert_eq!(
            response.body.content,
            [ContentBlock::text("Some words here")]
        );
    }

    #[tokio::test]
    async fn surfaces_error_event() {
        let server = MockSse::start(Script::sse([
            event(message_start(5)),
            event(text_start(0)),
            event(text_delta(0, "par")),
            event(json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}})),
        ]))
        .await;

        let response = non_streaming_messages(&server.messages_url, non_streaming_request(), None)
            .await
            .unwrap();

        assert_eq!(response.error.unwrap()["message"], json!("Overloaded"));
        assert_eq!(response.body.content, [ContentBlock::text("par")]);
        assert!(response.stats.unwrap().error.is_some());
    }
}

mod reasoning_remapping {
    use super::*;
    use llm_shim::apis::messages::models::lib::options::reasoning_remapping::MessagesReasoningPosition;

    #[tokio::test]
    async fn thinking_blocks_become_tags_in_the_text_stream() {
        let server = MockSse::start(Script::sse([
            event(message_start(10)),
            event(thinking_start(0)),
            event(thinking_delta(0, "plan")),
            event(signature_delta(0, "sig")),
            event(block_stop(0)),
            event(text_start(1)),
            event(text_delta(1, "answer")),
            event(block_stop(1)),
            event(message_delta("end_turn", 5)),
            event(message_stop()),
        ]))
        .await;

        let options = MessagesOptions::default().with_reasoning_remapping((
            MessagesReasoningPosition::ThinkingBlock,
            MessagesReasoningPosition::text_unchecked("<think>", "</think>"),
        ));

        let events = collect_with(&server, request(json!({})), options).await;

        let raw = raw_events(&events);
        assert_eq!(texts(&raw), "<think>plan</think>answer");
        assert!(raw.iter().all(|e| !matches!(
            e,
            MessagesStreamEvent::ContentBlockStart {
                content_block: ContentBlock::Thinking { .. },
                ..
            }
        )));
        let stops: Vec<_> = raw
            .iter()
            .filter(|e| matches!(e, MessagesStreamEvent::ContentBlockStop { .. }))
            .collect();
        assert_eq!(stops.len(), 1);
        assert_eq!(done_stats(&events).output_tokens, 5);
    }

    #[tokio::test]
    async fn tags_become_a_thinking_block_in_the_folded_response() {
        let server = MockSse::start(Script::sse([
            event(message_start(10)),
            event(text_start(0)),
            event(text_delta(0, "<thi")),
            event(text_delta(0, "nk>plan")),
            event(text_delta(0, "ning</think>ans")),
            event(text_delta(0, "wer")),
            event(block_stop(0)),
            event(message_delta("end_turn", 5)),
            event(message_stop()),
        ]))
        .await;

        let options = MessagesOptions::default().with_reasoning_remapping((
            MessagesReasoningPosition::text_unchecked("<think>", "</think>"),
            MessagesReasoningPosition::ThinkingBlock,
        ));

        let response =
            non_streaming_messages(&server.messages_url, non_streaming_request(), options)
                .await
                .unwrap();

        assert_eq!(
            response.body.content,
            [
                ContentBlock::thinking("planning", ""),
                ContentBlock::text("answer")
            ]
        );
    }
}

mod throttling_and_dispatch {
    use super::*;
    use async_trait::async_trait;
    use llm_shim::{
        apis::messages::{messages, models::api::response::MessagesResponse},
        traits::max_tps::MaxTps,
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
    async fn slows_the_stream_down_to_max_tps_using_estimated_tokens() {
        const MAX_TPS: f32 = 100.0;
        let chunk = "twelve bytes"; // 12 bytes -> ceil(12 / 3.5) = 4 estimated tokens
        let server = MockSse::start(simple_text_script(&[chunk; 5], 3, 20)).await;

        let throttler = FixedTps(MAX_TPS);
        let options = MessagesOptions::default().with_tps_throttler(&throttler);

        let started = Instant::now();
        let events = collect_with(&server, request(json!({})), options).await;
        let elapsed = started.elapsed();

        let stats = done_stats(&events);
        let estimated_total: u32 = stats.chunks.iter().map(|c| c.tokens).sum();
        assert_eq!(estimated_total, 20);
        let floor = Duration::from_secs_f32(estimated_total as f32 / MAX_TPS);
        assert!(
            elapsed >= floor - Duration::from_millis(20),
            "elapsed {elapsed:?} < {floor:?}"
        );
        assert!(stats.tps_avg().unwrap() <= MAX_TPS + 1.0);
    }

    #[tokio::test]
    async fn top_level_function_dispatches_on_the_stream_flag() {
        let server = MockSse::start(simple_text_script(&["hi"], 1, 1)).await;

        let response = messages(&server.messages_url, non_streaming_request(), None)
            .await
            .unwrap();
        let MessagesResponse::NonStreaming(response) = response else {
            panic!("expected non-streaming");
        };
        assert_eq!(response.body.content, [ContentBlock::text("hi")]);

        let response = messages(&server.messages_url, request(json!({})), None)
            .await
            .unwrap();
        assert!(matches!(response, MessagesResponse::Streaming(_)));
    }
}
