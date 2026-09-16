mod support;

use async_trait::async_trait;
use llm_shim::{
    apis::transcriptions::{
        models::{
            api::{
                request::{AudioFile, TranscriptionsFields, TranscriptionsRequest},
                response::{
                    TranscriptionsResponse, non_streaming::TranscriptionsResponseBody,
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
        TranscriptionsOptions::default().with_bearer_token("tok"),
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
    assert_eq!(
        body.usage.as_ref().unwrap().additional_properties["seconds"],
        json!(4)
    );
    assert_eq!(body.additional_properties["language"], json!("english"));
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
        TranscriptionsOptions::default().with_tps_throttler(&throttler),
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(450));
    assert!(response.stats.chunks[0].tps_correction_duration.is_some());
}
