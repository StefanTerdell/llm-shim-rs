mod support;

use llm_shim::credentials::Credentials;

use async_trait::async_trait;
use llm_shim::{
    apis::transcriptions::{
        models::{
            api::{
                request::{AudioFile, TranscriptionsFields, TranscriptionsRequest},
                response::{
                    TranscriptionsResponse,
                    non_streaming::{TranscriptionsResponseBody, TranscriptionsUsageType},
                    streaming::TranscriptionsStreamEvent,
                },
            },
            lib::{
                options::TranscriptionsOptions, streaming::response::StreamingTranscriptionsEvent,
            },
        },
        non_streaming::non_streaming_transcriptions,
        streaming::streaming_transcriptions,
        transcriptions,
    },
    stats::StreamStats,
    traits::max_tps::MaxTps,
};
use serde_json::json;
use std::time::{Duration, Instant};
use support::*;
use tokio_stream::StreamExt;

const FAKE_WAV: &[u8] = b"RIFF....WAVEfmt fake audio bytes";

fn fields(extra: serde_json::Value) -> TranscriptionsFields {
    let mut body = json!({"model": "gpt-4o-transcribe"});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

fn request(extra: serde_json::Value) -> TranscriptionsRequest {
    TranscriptionsRequest::new(
        AudioFile::bytes("clip.wav", FAKE_WAV).with_mime("audio/wav"),
        fields(extra),
    )
}

fn tokens_usage(input: u32, output: u32) -> serde_json::Value {
    json!({"type": "tokens", "input_tokens": input, "output_tokens": output, "total_tokens": input + output, "input_token_details": {"text_tokens": 0, "audio_tokens": input}})
}

fn event(json: serde_json::Value) -> Frame {
    let event_type = json["type"].as_str().unwrap().to_owned();
    raw(format!("event: {event_type}\ndata: {json}\n\n"))
}

struct FixedTps(f32);

#[async_trait]
impl MaxTps for FixedTps {
    async fn get(&self) -> Option<f32> {
        Some(self.0)
    }
}

fn done_stats(events: &[StreamingTranscriptionsEvent]) -> &StreamStats {
    match events.last().unwrap() {
        StreamingTranscriptionsEvent::Done { stats } => stats,
        _ => panic!("last event should be Done"),
    }
}

#[tokio::test]
async fn sends_the_file_and_fields_as_multipart_and_parses_json() {
    let server = MockSse::start(Script::json(
        json!({"text": "Hello world", "usage": tokens_usage(14, 3)}),
    ))
    .await;

    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(json!({"language": "en", "temperature": 0, "timestamp_granularities": ["word", "segment"], "chunking_strategy": {"type": "server_vad"}})),
        TranscriptionsOptions::default().with_credentials(Credentials::bearer("tok")),
    )
    .await
    .unwrap();

    let TranscriptionsResponseBody::Json(body) = &response.body else {
        panic!("expected a JSON body");
    };
    assert_eq!(body.text, "Hello world");
    assert_eq!(body.usage.as_ref().unwrap().input_tokens, Some(14));
    assert!(!response.stats.input_tokens_is_estimate);
    assert_eq!(response.stats.input_tokens, 14);
    assert!(!response.stats.output_tokens_is_estimate);
    assert_eq!(response.stats.output_tokens, 3);
    assert_eq!(response.stats.chunks.len(), 1);
    assert_eq!(response.stats.chunks[0].tokens, 3);

    let sent = server.single_request();
    assert_eq!(sent["file"]["filename"], json!("clip.wav"));
    assert_eq!(sent["file"]["content_type"], json!("audio/wav"));
    assert_eq!(
        sent["file"]["content"],
        json!(String::from_utf8_lossy(FAKE_WAV))
    );
    assert_eq!(sent["model"], json!("gpt-4o-transcribe"));
    assert_eq!(sent["language"], json!("en"));
    assert_eq!(sent["temperature"], json!("0"));
    assert_eq!(sent["timestamp_granularities"], json!(["word", "segment"]));
    assert_eq!(
        sent["chunking_strategy"],
        json!("{\"type\":\"server_vad\"}")
    );
    assert!(sent.get("stream").is_none(), "sent: {sent}");

    let headers = server.single_request_headers();
    assert_eq!(headers["authorization"], "Bearer tok");
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("multipart/form-data")
    );
}

#[tokio::test]
async fn plain_text_formats_come_back_as_text_with_estimated_tokens() {
    let server = MockSse::start(Script::text(
        "1\n00:00:00,000 --> 00:00:01,000\nHello world\n",
    ))
    .await;

    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(json!({"response_format": "srt"})),
        None,
    )
    .await
    .unwrap();

    let TranscriptionsResponseBody::Text(text) = &response.body else {
        panic!("expected a text body");
    };
    assert!(text.starts_with("1\n"));
    assert!(response.stats.output_tokens_is_estimate);
    assert!(response.stats.output_tokens > 0);
    assert!(response.stats.input_tokens_is_estimate);
    assert_eq!(server.single_request()["response_format"], json!("srt"));
}

#[tokio::test]
async fn duration_usage_leaves_token_counts_estimated_and_passes_through() {
    let server = MockSse::start(Script::json(json!({"text": "Hello", "usage": {"type": "duration", "seconds": 4}, "language": "english", "duration": 4.0, "segments": []}))).await;

    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(json!({"model": "whisper-1"})),
        None,
    )
    .await
    .unwrap();

    let TranscriptionsResponseBody::Json(body) = &response.body else {
        panic!("expected a JSON body");
    };
    let usage = body.usage.as_ref().unwrap();
    assert_eq!(usage.usage_type, Some(TranscriptionsUsageType::Duration));
    assert_eq!(usage.seconds, Some(4.0));
    assert_eq!(body.duration, Some(4.0));
    assert_eq!(body.language.as_deref(), Some("english"));
    assert!(response.stats.input_tokens_is_estimate);
    assert!(response.stats.output_tokens_is_estimate);
    assert!(response.stats.output_tokens > 0);
}

#[tokio::test]
async fn streams_deltas_then_done_with_exact_usage() {
    let server = MockSse::start(Script::sse([
        event(json!({"type": "transcript.text.delta", "delta": "Hello", "logprobs": []})),
        event(json!({"type": "transcript.text.delta", "delta": " world"})),
        event(json!({"type": "transcript.text.segment", "id": "seg_0", "start": 0.0, "end": 1.2, "text": "Hello world", "speaker": "A"})),
        event(json!({"type": "transcript.text.done", "text": "Hello world", "usage": tokens_usage(14, 3)})),
    ]))
    .await;

    let mut stream = streaming_transcriptions(&server.transcriptions_url, request(json!({})), None)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert_eq!(events.len(), 5);
    let deltas: String = events
        .iter()
        .filter_map(|e| match e {
            StreamingTranscriptionsEvent::Event {
                event: TranscriptionsStreamEvent::Delta { delta, .. },
            } => Some(delta.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, "Hello world");
    assert!(matches!(
        &events[2],
        StreamingTranscriptionsEvent::Event {
            event: TranscriptionsStreamEvent::Other(_)
        }
    ));
    assert!(matches!(
        &events[3],
        StreamingTranscriptionsEvent::Event {
            event: TranscriptionsStreamEvent::Done { .. }
        }
    ));

    let stats = done_stats(&events);
    assert!(!stats.input_tokens_is_estimate);
    assert_eq!(stats.input_tokens, 14);
    assert!(!stats.output_tokens_is_estimate);
    assert_eq!(stats.output_tokens, 3);
    assert_eq!(stats.chunks.len(), 2);

    assert_eq!(server.single_request()["stream"], json!("true"));
}

#[tokio::test]
async fn streams_a_file_from_disk_without_buffering_it_first() {
    let dir = std::env::temp_dir().join(format!("llm-shim-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("speech.mp3");
    std::fs::write(&path, FAKE_WAV).unwrap();

    let server = MockSse::start(Script::sse([
        event(json!({"type": "transcript.text.delta", "delta": "ok"})),
        event(json!({"type": "transcript.text.done", "text": "ok"})),
    ]))
    .await;

    let request =
        TranscriptionsRequest::new(AudioFile::path(&path).await.unwrap(), fields(json!({})));
    let mut stream = streaming_transcriptions(&server.transcriptions_url, request, None)
        .await
        .unwrap();
    while let Some(event) = stream.next().await {
        event.unwrap();
    }

    let sent = server.single_request();
    assert_eq!(sent["file"]["filename"], json!("speech.mp3"));
    assert_eq!(sent["file"]["size"], json!(FAKE_WAV.len()));
    assert_eq!(
        sent["file"]["content"],
        json!(String::from_utf8_lossy(FAKE_WAV))
    );
    let content_length = server
        .single_request_headers()
        .get("content-length")
        .cloned();
    assert!(
        content_length.is_some(),
        "a file with a known size should produce a Content-Length"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn error_event_ends_the_stream_with_event_error() {
    let server = MockSse::start(Script::sse([
        event(json!({"type": "transcript.text.delta", "delta": "Hel"})),
        event(json!({"type": "error", "error": {"message": "boom", "type": "server_error"}})),
        event(json!({"type": "transcript.text.delta", "delta": "lo"})),
    ]))
    .await;

    let mut stream = streaming_transcriptions(&server.transcriptions_url, request(json!({})), None)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert_eq!(events.len(), 2);
    let StreamingTranscriptionsEvent::EventError { stats, .. } = &events[1] else {
        panic!("second event should be EventError");
    };
    assert!(stats.error.as_deref().unwrap().contains("boom"));
}

#[tokio::test]
async fn top_level_function_dispatches_on_the_stream_field() {
    let server = MockSse::start(Script::json(json!({"text": "hi"}))).await;
    let response = transcriptions(&server.transcriptions_url, request(json!({})), None)
        .await
        .unwrap();
    assert!(matches!(response, TranscriptionsResponse::NonStreaming(_)));

    let server = MockSse::start(Script::sse([event(
        json!({"type": "transcript.text.done", "text": "hi"}),
    )]))
    .await;
    let response = transcriptions(
        &server.transcriptions_url,
        request(json!({"stream": true})),
        None,
    )
    .await
    .unwrap();
    assert!(matches!(response, TranscriptionsResponse::Streaming(_)));
}

#[tokio::test]
async fn non_streaming_sleeps_when_the_request_exceeded_max_tps() {
    let server = MockSse::start(Script::json(
        json!({"text": "x", "usage": tokens_usage(10, 50)}),
    ))
    .await;
    let throttler = FixedTps(100.0);

    let started = Instant::now();
    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(json!({})),
        TranscriptionsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(450));
    assert!(response.stats.chunks[0].tps_correction_duration.is_some());
}

fn vllm_chunk(content: &str) -> Frame {
    data(
        json!({"id": "t-1", "object": "transcription.chunk", "model": "whisper-large-v3", "choices": [{"delta": {"content": content}}]}),
    )
}

async fn collect_events(
    url: &str,
    options: TranscriptionsOptions<'_>,
) -> Vec<StreamingTranscriptionsEvent> {
    let mut stream = streaming_transcriptions(url, request(json!({})), options)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    events
}

fn stream_events(events: &[StreamingTranscriptionsEvent]) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter_map(|e| match e {
            StreamingTranscriptionsEvent::Event { event } => {
                Some(serde_json::to_value(event).unwrap())
            }
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn normalizes_vllm_chat_chunks_into_delta_and_done_events() {
    let server = MockSse::start(Script::sse([
        vllm_chunk(" Hello"),
        vllm_chunk(","),
        data(json!({"object": "transcription.chunk", "choices": [{"delta": {"content": ""}, "finish_reason": "stop"}]})),
        data(json!({"object": "transcription.chunk", "choices": [], "usage": {"prompt_tokens": 12, "completion_tokens": 2, "total_tokens": 14}})),
        done(),
    ]))
    .await;

    let events = collect_events(
        &server.transcriptions_url,
        TranscriptionsOptions::default().with_normalized_events(),
    )
    .await;

    assert_eq!(
        stream_events(&events),
        vec![
            json!({"type": "transcript.text.delta", "delta": " Hello"}),
            json!({"type": "transcript.text.delta", "delta": ","}),
            json!({"type": "transcript.text.done", "text": " Hello,", "usage": {"type": "tokens", "input_tokens": 12, "output_tokens": 2, "total_tokens": 14}}),
        ]
    );

    let stats = done_stats(&events);
    assert_eq!(stats.input_tokens, 12);
    assert_eq!(stats.output_tokens, 2);
    assert!(!stats.output_tokens_is_estimate);
}

#[tokio::test]
async fn normalizes_a_json_body_returned_in_place_of_a_stream() {
    let server = MockSse::start(Script::json(json!({"text": "Hello there", "usage": {"type": "duration", "seconds": 4}, "duration": 3.6}))).await;

    let events = collect_events(
        &server.transcriptions_url,
        TranscriptionsOptions::default().with_normalized_events(),
    )
    .await;

    assert_eq!(
        stream_events(&events),
        vec![
            json!({"type": "transcript.text.delta", "delta": "Hello there"}),
            json!({"type": "transcript.text.done", "text": "Hello there", "usage": {"type": "duration", "seconds": 4.0}}),
        ]
    );
    assert!(done_stats(&events).output_tokens > 0);
    assert_eq!(server.single_request()["stream"], json!("true"));
}

#[tokio::test]
async fn normalizes_bare_text_frames() {
    let server = MockSse::start(Script::sse([
        data(json!({"text": "Hello there", "id": "abc"})),
        done(),
    ]))
    .await;

    let events = collect_events(
        &server.transcriptions_url,
        TranscriptionsOptions::default().with_normalized_events(),
    )
    .await;

    assert_eq!(
        stream_events(&events),
        vec![
            json!({"type": "transcript.text.delta", "delta": "Hello there"}),
            json!({"type": "transcript.text.done", "text": "Hello there"}),
        ]
    );
}

#[tokio::test]
async fn normalization_leaves_openai_events_untouched() {
    let server = MockSse::start(Script::sse([
        event(json!({"type": "transcript.text.delta", "delta": "Hi"})),
        event(json!({"type": "transcript.text.segment", "id": "seg_0", "text": "Hi"})),
        event(json!({"type": "transcript.text.done", "text": "Hi", "usage": tokens_usage(5, 1)})),
    ]))
    .await;

    let events = collect_events(
        &server.transcriptions_url,
        TranscriptionsOptions::default().with_normalized_events(),
    )
    .await;

    assert_eq!(
        stream_events(&events),
        vec![
            json!({"type": "transcript.text.delta", "delta": "Hi"}),
            json!({"type": "transcript.text.segment", "id": "seg_0", "text": "Hi"}),
            json!({"type": "transcript.text.done", "text": "Hi", "usage": tokens_usage(5, 1)}),
        ]
    );
}

#[tokio::test]
async fn without_normalization_vllm_chunks_pass_through_as_other() {
    let server = MockSse::start(Script::sse([vllm_chunk("Hi"), done()])).await;

    let events = collect_events(&server.transcriptions_url, TranscriptionsOptions::default()).await;

    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        StreamingTranscriptionsEvent::Event {
            event: TranscriptionsStreamEvent::Other(_)
        }
    ));
}

#[tokio::test]
async fn without_normalization_a_json_body_passes_through_as_one_event() {
    let body = json!({"text": "Hello there", "id": "abc"});
    let server = MockSse::start(Script::json(body.clone())).await;

    let events = collect_events(&server.transcriptions_url, TranscriptionsOptions::default()).await;

    assert_eq!(stream_events(&events), vec![body]);
    assert!(done_stats(&events).output_tokens > 0);
}

#[tokio::test]
async fn normalization_bills_a_body_without_usage_by_its_duration() {
    let server = MockSse::start(Script::json(json!({"text": "Hi", "duration": 2.5}))).await;

    let events = collect_events(
        &server.transcriptions_url,
        TranscriptionsOptions::default().with_normalized_events(),
    )
    .await;

    assert_eq!(
        stream_events(&events)[1],
        json!({"type": "transcript.text.done", "text": "Hi", "usage": {"type": "duration", "seconds": 2.5}})
    );
}

fn verbose_body() -> serde_json::Value {
    json!({
        "id": "vox-1",
        "task": "transcribe",
        "language": "sv",
        "duration": 4.5705625,
        "text": " Hello there. General Kenobi.",
        "segments": [
            {"id": 0, "start": 0.0, "end": 1.5, "text": " Hello there.", "tokens": [1, 2]},
            {"id": 1, "start": 1.5, "end": 3661.0045, "text": " General Kenobi.", "tokens": [3]}
        ]
    })
}

async fn verbose_transcription(
    format: Option<&str>,
) -> (
    llm_shim::apis::transcriptions::models::api::response::non_streaming::NonStreamingTranscriptionsResponse,
    serde_json::Value,
){
    let server = MockSse::start(Script::json(verbose_body())).await;
    let extra = format.map_or(json!({}), |format| json!({"response_format": format}));
    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(extra),
        TranscriptionsOptions::default().with_upstream_verbose_json(),
    )
    .await
    .unwrap();
    (response, server.single_request())
}

fn text_body(body: &TranscriptionsResponseBody) -> &str {
    match body {
        TranscriptionsResponseBody::Text(text) => text,
        TranscriptionsResponseBody::Json(_) => panic!("expected a text body"),
    }
}

#[tokio::test]
async fn upstream_verbose_json_renders_json_without_the_verbose_fields() {
    let (response, sent) = verbose_transcription(None).await;

    assert_eq!(sent["response_format"], json!("verbose_json"));
    let TranscriptionsResponseBody::Json(body) = &response.body else {
        panic!("expected a JSON body");
    };
    assert_eq!(
        serde_json::to_value(body).unwrap(),
        json!({"id": "vox-1", "text": " Hello there. General Kenobi.", "usage": {"type": "duration", "seconds": 4.5705625}})
    );
    assert_eq!(response.usage.unwrap().seconds, Some(4.5705625));
}

#[tokio::test]
async fn upstream_verbose_json_renders_text() {
    let (response, sent) = verbose_transcription(Some("text")).await;

    assert_eq!(sent["response_format"], json!("verbose_json"));
    assert_eq!(text_body(&response.body), " Hello there. General Kenobi.");
    assert_eq!(response.usage.unwrap().seconds, Some(4.5705625));
}

#[tokio::test]
async fn upstream_verbose_json_renders_srt_from_the_segments() {
    let (response, _) = verbose_transcription(Some("srt")).await;

    assert_eq!(
        text_body(&response.body),
        "1\n00:00:00,000 --> 00:00:01,500\nHello there.\n\n2\n00:00:01,500 --> 01:01:01,005\nGeneral Kenobi.\n\n"
    );
    assert!(response.usage.is_some());
}

#[tokio::test]
async fn upstream_verbose_json_renders_vtt_from_the_segments() {
    let (response, _) = verbose_transcription(Some("vtt")).await;

    assert_eq!(
        text_body(&response.body),
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.500\nHello there.\n\n00:00:01.500 --> 01:01:01.005\nGeneral Kenobi.\n\n"
    );
}

#[tokio::test]
async fn upstream_verbose_json_keeps_a_requested_verbose_body_and_adds_usage() {
    let (response, _) = verbose_transcription(Some("verbose_json")).await;

    let TranscriptionsResponseBody::Json(body) = &response.body else {
        panic!("expected a JSON body");
    };
    let mut expected = verbose_body();
    expected["usage"] = json!({"type": "duration", "seconds": 4.5705625});
    assert_eq!(serde_json::to_value(body).unwrap(), expected);
}

#[tokio::test]
async fn upstream_verbose_json_leaves_diarized_json_alone() {
    let (_, sent) = verbose_transcription(Some("diarized_json")).await;

    assert_eq!(sent["response_format"], json!("diarized_json"));
}

#[tokio::test]
async fn upstream_verbose_json_passes_a_plain_text_answer_through() {
    let server = MockSse::start(Script::text("Hello there")).await;
    let response = non_streaming_transcriptions(
        &server.transcriptions_url,
        request(json!({"response_format": "text"})),
        TranscriptionsOptions::default().with_upstream_verbose_json(),
    )
    .await
    .unwrap();

    assert_eq!(text_body(&response.body), "Hello there");
    assert!(response.usage.is_none());
}
