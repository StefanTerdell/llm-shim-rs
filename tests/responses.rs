mod support;

use async_trait::async_trait;
use llm_shim::traits::max_tps::MaxTps;
use llm_shim::{
    apis::responses::{
        models::{
            api::{
                common::{ContentPart, OutputItem, ReasoningPart},
                request::streaming::StreamingResponsesRequestBody,
                response::streaming::ResponsesStreamEvent,
            },
            lib::{options::ResponsesOptions, streaming::response::StreamingResponsesEvent},
        },
        streaming::streaming_responses,
        virtual_non_streaming::virtual_non_streaming_responses,
    },
    stats::StreamStats,
};
use serde_json::json;
use support::{openai_responses::*, *};
use tokio_stream::StreamExt;

struct FixedTps(f32);

#[async_trait]
impl MaxTps for FixedTps {
    async fn get(&self) -> Option<f32> {
        Some(self.0)
    }
}

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
            virtual_non_streaming_responses(&server.responses_url, non_streaming_request(), None)
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
            virtual_non_streaming_responses(&server.responses_url, non_streaming_request(), None)
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
            virtual_non_streaming_responses(&server.responses_url, non_streaming_request(), None)
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
    use llm_shim::apis::responses::models::lib::options::reasoning_remapping::ResponsesReasoningPosition;

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
            ResponsesReasoningPosition::Summary,
            ResponsesReasoningPosition::text_unchecked("<think>", "</think>"),
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
            ResponsesReasoningPosition::text_unchecked("<think>", "</think>"),
            ResponsesReasoningPosition::Summary,
        ));

        let response = virtual_non_streaming_responses(
            &server.responses_url,
            non_streaming_request(),
            options,
        )
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
    use llm_shim::{
        apis::responses::{models::api::response::ResponsesResponse, responses},
        non_streaming_mode::NonStreamingMode,
    };
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn slows_the_stream_down_to_max_tps_using_estimated_tokens() {
        const MAX_TPS: f32 = 100.0;
        let chunk = "twelve bytes"; // 12 bytes -> ceil(12 / 3.5) = 4 estimated tokens
        let server = MockSse::start(simple_text_script(&[chunk; 5], 3, 20)).await;

        let throttler = FixedTps(MAX_TPS);
        let options = ResponsesOptions::default().with_max_tps(&throttler);

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
    async fn top_level_function_streams_and_assembles_non_streaming_requests_by_default() {
        let server = MockSse::start(Script::sse([
            event(created()),
            event(completed(
                json!([message_item("msg_1", "hi", "completed")]),
                1,
                1,
            )),
        ]))
        .await;

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
        assert_eq!(server.single_request()["stream"], json!(true));
    }

    #[tokio::test]
    async fn top_level_function_sends_a_real_non_streaming_request_when_asked() {
        let server = MockSse::start(Script::json(response_object(
            "completed",
            json!([message_item("msg_1", "hi", "completed")]),
            Some((1, 1)),
        )))
        .await;

        let options = llm_shim::apis::responses::models::lib::options::ResponsesOptions::default()
            .with_non_streaming_mode(llm_shim::non_streaming_mode::NonStreamingMode::Real);
        let response = responses(&server.responses_url, non_streaming_request(), options)
            .await
            .unwrap();
        let ResponsesResponse::NonStreaming(response) = response else {
            panic!("expected non-streaming");
        };
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("hi"));
    }

    #[tokio::test]
    async fn top_level_function_streams_when_asked() {
        let server = MockSse::start(simple_text_script(&["hi"], 1, 1)).await;

        let response = responses(&server.responses_url, request(json!({})), None)
            .await
            .unwrap();
        assert!(matches!(response, ResponsesResponse::Streaming(_)));
    }

    #[tokio::test]
    async fn top_level_function_can_be_told_to_fold_a_stream() {
        let server = MockSse::start(simple_text_script(&["hi"], 1, 1)).await;

        let options =
            ResponsesOptions::default().with_non_streaming_mode(NonStreamingMode::Virtual);
        let response = responses(&server.responses_url, non_streaming_request(), options)
            .await
            .unwrap();
        let ResponsesResponse::NonStreaming(response) = response else {
            panic!("expected non-streaming");
        };
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("hi"));
    }
}

mod real_non_streaming {
    use super::*;
    use llm_shim::apis::responses::{
        models::lib::options::reasoning_remapping::ResponsesReasoningPosition,
        non_streaming::non_streaming_responses,
    };
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn sends_one_request_and_passes_the_body_through() {
        let server = MockSse::start(Script::json(response_object(
            "completed",
            json!([message_item("msg_1", "hi", "completed")]),
            Some((7, 4)),
        )))
        .await;

        let response = non_streaming_responses(
            &server.responses_url,
            non_streaming_request(),
            ResponsesOptions::default().with_bearer_token("tok"),
        )
        .await
        .unwrap();

        let sent = server.single_request();
        assert!(
            sent.get("stream").is_none_or(|s| s == &json!(false)),
            "sent: {sent}"
        );
        assert_eq!(
            server.single_request_headers()["authorization"],
            "Bearer tok"
        );

        assert_eq!(response.body.additional_properties["id"], json!("resp_1"));
        assert_eq!(
            response.body.additional_properties["status"],
            json!("completed")
        );
        let OutputItem::Message { content, .. } = &response.body.output[0] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("hi"));
        assert!(response.error.is_none());

        let stats = response.stats.unwrap();
        assert!(!stats.input_tokens_is_estimate);
        assert_eq!(stats.input_tokens, 7);
        assert!(!stats.output_tokens_is_estimate);
        assert_eq!(stats.output_tokens, 4);
        assert_eq!(stats.chunks[0].tokens, 4);
    }

    #[tokio::test]
    async fn estimates_and_fills_usage_when_the_server_sends_none() {
        let server = MockSse::start(Script::json(response_object(
            "completed",
            json!([message_item("msg_1", "hello there world", "completed")]),
            None,
        )))
        .await;

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), None)
                .await
                .unwrap();

        let stats = response.stats.unwrap();
        assert!(stats.input_tokens_is_estimate);
        assert!(stats.output_tokens_is_estimate);
        assert!(stats.output_tokens > 0);
        let usage = response.body.usage.flatten().unwrap();
        assert_eq!(usage.output_tokens, Some(stats.output_tokens));
        assert_eq!(usage.input_tokens, Some(stats.input_tokens));
        assert_eq!(
            usage.total_tokens,
            Some(stats.input_tokens + stats.output_tokens)
        );
    }

    #[tokio::test]
    async fn sleeps_when_the_request_exceeded_max_tps() {
        let server = MockSse::start(Script::json(response_object(
            "completed",
            json!([message_item("msg_1", "x", "completed")]),
            Some((1, 50)),
        )))
        .await;
        let throttler = FixedTps(100.0);

        let started = Instant::now();
        let response = non_streaming_responses(
            &server.responses_url,
            non_streaming_request(),
            ResponsesOptions::default().with_max_tps(&throttler),
        )
        .await
        .unwrap();

        assert!(started.elapsed() >= Duration::from_millis(450));
        assert!(
            response.stats.unwrap().chunks[0]
                .tps_correction_duration
                .is_some()
        );
    }

    #[tokio::test]
    async fn surfaces_the_error_field_of_a_failed_response() {
        let mut body = response_object("failed", json!([]), None);
        body["error"] = json!({"code": "server_error", "message": "boom"});
        let server = MockSse::start(Script::json(body)).await;

        let response =
            non_streaming_responses(&server.responses_url, non_streaming_request(), None)
                .await
                .unwrap();

        assert_eq!(response.error.unwrap()["message"], json!("boom"));
        assert!(response.stats.unwrap().error.is_some());
    }

    #[tokio::test]
    async fn remaps_tags_into_a_reasoning_item() {
        let server = MockSse::start(Script::json(response_object(
            "completed",
            json!([message_item(
                "msg_1",
                "<think>plan</think>answer",
                "completed"
            )]),
            Some((1, 1)),
        )))
        .await;

        let options = ResponsesOptions::default().with_reasoning_remapping((
            ResponsesReasoningPosition::text_unchecked("<think>", "</think>"),
            ResponsesReasoningPosition::Summary,
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
        let OutputItem::Message { content, .. } = &response.body.output[1] else {
            panic!("message")
        };
        assert_eq!(content[0], ContentPart::output_text("answer"));
    }
}
