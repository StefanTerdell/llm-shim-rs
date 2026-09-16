mod support;

use llm_stream_map::{
    responses::{
        models::{
            api::{
                common::{ContentPart, OutputItem, ReasoningPart},
                request::streaming::StreamingResponsesRequestBody,
                response::streaming::ResponsesStreamEvent,
            },
            lib::{options::ResponsesOptions, streaming::response::StreamingResponsesEvent},
        },
        non_streaming::non_streaming_responses,
        streaming::streaming_responses,
    },
    stats::StreamStats,
};
use serde_json::json;
use support::{openai_responses::*, *};
use tokio_stream::StreamExt;

async fn collect(
    server: &MockSse,
    body: StreamingResponsesRequestBody,
) -> Vec<StreamingResponsesEvent> {
    collect_with(server, body, None).await
}

async fn collect_with<'a>(
    server: &MockSse,
    body: StreamingResponsesRequestBody,
    options: impl Into<Option<ResponsesOptions<'a>>>,
) -> Vec<StreamingResponsesEvent> {
    let mut stream = streaming_responses(&server.responses_url, body, options)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

fn raw_events(events: &[StreamingResponsesEvent]) -> Vec<&ResponsesStreamEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            StreamingResponsesEvent::Event { event }
            | StreamingResponsesEvent::EventError { event, .. } => Some(event),
            _ => None,
        })
        .collect()
}

fn done_stats(events: &[StreamingResponsesEvent]) -> &StreamStats {
    match events.last().unwrap() {
        StreamingResponsesEvent::Done { stats } => stats,
        _ => panic!("last event should be Done"),
    }
}

fn text_deltas(events: &[&ResponsesStreamEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            ResponsesStreamEvent::OutputTextDelta { delta, .. } => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn passes_events_through_and_ends_with_done() {
    let server = MockSse::start(simple_text_script(&["Hel", "lo"], 25, 12)).await;

    let events = collect(&server, request(json!({}))).await;

    let raw = raw_events(&events);
    assert_eq!(raw.len(), 9);
    assert!(matches!(raw[0], ResponsesStreamEvent::Created { .. }));
    assert!(matches!(raw[8], ResponsesStreamEvent::Completed { .. }));
    assert_eq!(text_deltas(&raw), "Hello");

    let stats = done_stats(&events);
    assert!(!stats.input_tokens_is_estimate);
    assert_eq!(stats.input_tokens, 25);
    assert!(!stats.output_tokens_is_estimate);
    assert_eq!(stats.output_tokens, 12);
    assert!(stats.error.is_none());
    assert_eq!(server.single_request()["stream"], json!(true));
}

#[tokio::test]
async fn estimates_tokens_from_deltas_until_completed_arrives() {
    let server = MockSse::start(Script::sse([
        event(created()),
        event(item_added(0, reasoning_item("rs_1", &[]))),
        event(reasoning_text_delta(
            "rs_1",
            0,
            "some raw reasoning text here",
        )),
        event(item_added(1, message_item("msg_1", "", "in_progress"))),
        event(text_delta("msg_1", 1, "and an answer")),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    let stats = done_stats(&events);
    assert!(stats.output_tokens_is_estimate);
    assert!(stats.input_tokens_is_estimate);
    assert!(stats.output_tokens > 0);
    let counted: Vec<_> = stats.chunks.iter().filter(|c| c.tokens > 0).collect();
    assert_eq!(counted.len(), 2);
}

#[tokio::test]
async fn sends_bearer_token() {
    let server = MockSse::start(simple_text_script(&["x"], 1, 1)).await;

    collect_with(
        &server,
        request(json!({})),
        ResponsesOptions::default().with_bearer_token("tok"),
    )
    .await;

    assert_eq!(
        server.single_request_headers()["authorization"],
        "Bearer tok"
    );
}

#[tokio::test]
async fn error_event_yields_event_error_and_ends_stream() {
    let server = MockSse::start(Script::sse([
        event(created()),
        event(json!({"type": "error", "code": "server_error", "message": "boom", "param": null, "sequence_number": 1})),
        event(item_added(0, message_item("msg_1", "", "in_progress"))),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    assert_eq!(events.len(), 2);
    let StreamingResponsesEvent::EventError { stats, event } = &events[1] else {
        panic!("second event should be EventError");
    };
    assert!(matches!(event, ResponsesStreamEvent::Error { .. }));
    assert!(stats.error.as_deref().unwrap().contains("boom"));
}

#[tokio::test]
async fn failed_response_yields_event_error_with_usage() {
    let mut failed = response_object("failed", json!([]), Some((5, 2)));
    failed["error"] = json!({"code": "server_error", "message": "exploded"});
    let server = MockSse::start(Script::sse([
        event(created()),
        event(json!({"type": "response.failed", "sequence_number": 1, "response": failed})),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    let StreamingResponsesEvent::EventError { stats, .. } = events.last().unwrap() else {
        panic!("last event should be EventError");
    };
    assert!(stats.error.as_deref().unwrap().contains("exploded"));
    assert_eq!(stats.output_tokens, 2);
}

#[tokio::test]
async fn incomplete_is_a_normal_terminal_event() {
    let server = MockSse::start(Script::sse([
        event(created()),
        event(json!({"type": "response.incomplete", "sequence_number": 1, "response": response_object("incomplete", json!([]), Some((5, 2)))})),
        event(json!({"type": "response.output_item.added", "output_index": 0, "item": {"type": "message", "id": "late"}})),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    assert_eq!(events.len(), 3);
    assert!(matches!(
        events.last().unwrap(),
        StreamingResponsesEvent::Done { .. }
    ));
}

#[tokio::test]
async fn unknown_events_and_data_only_frames_and_done_sentinel_are_tolerated() {
    let server = MockSse::start(Script::sse([
        data(created()),
        data(json!({"type": "response.web_search_call.searching", "output_index": 0, "item_id": "ws_1", "sequence_number": 1})),
        data(item_added(1, message_item("msg_1", "", "in_progress"))),
        data(text_delta("msg_1", 1, "hi")),
        data(completed(json!([message_item("msg_1", "hi", "completed")]), 3, 1)),
        done(),
    ]))
    .await;

    let events = collect(&server, request(json!({}))).await;

    let raw = raw_events(&events);
    assert!(matches!(raw[1], ResponsesStreamEvent::Other(_)));
    assert_eq!(text_deltas(&raw), "hi");
    assert_eq!(done_stats(&events).output_tokens, 1);
}

mod non_streaming {
    use super::*;

    #[tokio::test]
    async fn returns_the_completed_response_and_stats() {
        let server = MockSse::start(simple_text_script(&["Hel", "lo"], 25, 12)).await;

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), None)
                .await
                .unwrap();

        assert_eq!(response.body.additional_properties["id"], json!("resp_1"));
        assert_eq!(
            response.body.additional_properties["status"],
            json!("completed")
        );
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("Hello"));
        assert_eq!(
            response.body.usage.clone().flatten().unwrap().input_tokens,
            Some(25)
        );
        assert!(response.stats.is_some());
        assert!(response.error.is_none());
        assert_eq!(server.single_request()["stream"], json!(true));
    }

    #[tokio::test]
    async fn assembles_from_deltas_when_server_sends_no_done_events() {
        let server = MockSse::start(Script::sse([
            event(created()),
            event(item_added(0, reasoning_item("rs_1", &[]))),
            event(summary_delta("rs_1", 0, 0, "thin")),
            event(summary_delta("rs_1", 0, 0, "king")),
            event(item_added(1, message_item("msg_1", "", "in_progress"))),
            event(text_delta("msg_1", 1, "Hel")),
            event(text_delta("msg_1", 1, "lo")),
        ]))
        .await;

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), None)
                .await
                .unwrap();

        let OutputItem::Reasoning { summary, .. } = &response.body.output[0] else {
            panic!("reasoning")
        };
        assert_eq!(summary[0], ReasoningPart::summary_text("thinking"));
        let OutputItem::Message { content, .. } = &response.body.output[1] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("Hello"));
        let usage = response.body.usage.flatten().unwrap();
        assert!(usage.input_tokens.unwrap() > 0);
        assert!(usage.output_tokens.unwrap() > 0);
    }

    #[tokio::test]
    async fn surfaces_error_event() {
        let server = MockSse::start(Script::sse([
            event(created()),
            event(item_added(0, message_item("msg_1", "", "in_progress"))),
            event(text_delta("msg_1", 0, "par")),
            event(
                json!({"type": "error", "code": "server_error", "message": "boom", "param": null}),
            ),
        ]))
        .await;

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), None)
                .await
                .unwrap();

        assert_eq!(response.error.unwrap()["message"], json!("boom"));
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("par"));
        assert!(response.stats.unwrap().error.is_some());
    }
}

mod reasoning_remapping {
    use super::*;
    use llm_stream_map::responses::models::lib::options::reasoning_remapping::ReasoningPosition;

    #[tokio::test]
    async fn summaries_become_tags_in_the_text_stream_and_in_the_completed_output() {
        let server = MockSse::start(Script::sse([
            event(created()),
            event(item_added(0, reasoning_item("rs_1", &[]))),
            event(summary_part_added("rs_1", 0, 0)),
            event(summary_delta("rs_1", 0, 0, "plan")),
            event(summary_done("rs_1", 0, 0, "plan")),
            event(summary_part_done("rs_1", 0, 0, "plan")),
            event(item_done(0, reasoning_item("rs_1", &["plan"]))),
            event(item_added(1, message_item("msg_1", "", "in_progress"))),
            event(part_added("msg_1", 1, 0)),
            event(text_delta("msg_1", 1, "answer")),
            event(text_done("msg_1", 1, "answer")),
            event(part_done("msg_1", 1, 0, "answer")),
            event(item_done(1, message_item("msg_1", "answer", "completed"))),
            event(completed(
                json!([
                    reasoning_item("rs_1", &["plan"]),
                    message_item("msg_1", "answer", "completed")
                ]),
                10,
                5,
            )),
        ]))
        .await;

        let options = ResponsesOptions::default().with_reasoning_remapping((
            ReasoningPosition::Summary,
            ReasoningPosition::text_unchecked("<think>", "</think>"),
        ));

        let events = collect_with(&server, request(json!({})), options).await;

        let raw = raw_events(&events);
        assert_eq!(text_deltas(&raw), "<think>plan</think>answer");
        let ResponsesStreamEvent::Completed { response, .. } = raw.last().unwrap() else {
            panic!("last raw event should be completed");
        };
        assert_eq!(response.output.len(), 1);
        let OutputItem::Message { content, .. } = &response.output[0] else {
            panic!("message")
        };
        assert_eq!(
            content[0],
            ContentPart::output_text("<think>plan</think>answer")
        );
        assert_eq!(done_stats(&events).output_tokens, 5);
    }

    #[tokio::test]
    async fn tags_become_a_reasoning_item_in_the_folded_response() {
        let text = "<think>plan</think>answer";
        let server = MockSse::start(Script::sse([
            event(created()),
            event(item_added(0, message_item("msg_1", "", "in_progress"))),
            event(part_added("msg_1", 0, 0)),
            event(text_delta("msg_1", 0, "<thi")),
            event(text_delta("msg_1", 0, "nk>plan</think>ans")),
            event(text_delta("msg_1", 0, "wer")),
            event(text_done("msg_1", 0, text)),
            event(part_done("msg_1", 0, 0, text)),
            event(item_done(0, message_item("msg_1", text, "completed"))),
            event(completed(
                json!([message_item("msg_1", text, "completed")]),
                10,
                5,
            )),
        ]))
        .await;

        let options = ResponsesOptions::default().with_reasoning_remapping((
            ReasoningPosition::text_unchecked("<think>", "</think>"),
            ReasoningPosition::Summary,
        ));

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), options)
                .await
                .unwrap();

        assert_eq!(response.body.output.len(), 2);
        let OutputItem::Reasoning { summary, .. } = &response.body.output[0] else {
            panic!("reasoning")
        };
        assert_eq!(summary[0], ReasoningPart::summary_text("plan"));
        let OutputItem::Message { id, content, .. } = &response.body.output[1] else {
            panic!("message")
        };
        assert_eq!(id.as_deref(), Some("msg_1"));
        assert_eq!(content[0], ContentPart::output_text("answer"));
        assert_eq!(
            response.body.usage.clone().flatten().unwrap().output_tokens,
            Some(5)
        );
    }
}

mod throttling_and_dispatch {
    use super::*;
    use async_trait::async_trait;
    use llm_stream_map::{
        responses::{models::api::response::ResponsesResponse, responses},
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
        let options = ResponsesOptions::default().with_tps_throttler(&throttler);

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

        let response = responses(&server.responses_url, non_streaming_request(), None)
            .await
            .unwrap();
        let ResponsesResponse::NonStreaming(response) = response else {
            panic!("expected non-streaming");
        };
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("hi"));

        let response = responses(&server.responses_url, request(json!({})), None)
            .await
            .unwrap();
        assert!(matches!(response, ResponsesResponse::Streaming(_)));
    }
}
