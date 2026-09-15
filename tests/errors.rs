mod support;

use llm_stream_map::{
    chat_completion::{
        models::lib::streaming::response::StreamingChatCompletionEvent,
        streaming::streaming_chat_completion,
    },
    error::Error,
};
use reqwest::StatusCode;
use serde_json::json;
use support::*;
use tokio_stream::StreamExt;

#[tokio::test]
async fn non_2xx_response_surfaces_status_and_body() {
    let server = MockSse::start(Script::error(
        StatusCode::UNAUTHORIZED,
        json!({"error": {"message": "bad key", "type": "invalid_request_error"}}),
    ))
    .await;

    let result = streaming_chat_completion(&server.url, streaming_request(json!({})), None).await;

    match result {
        Err(Error::Reqwest(error)) => {
            assert_eq!(error.status(), Some(StatusCode::UNAUTHORIZED));
        }
        Err(other) => panic!("unexpected error variant: {other}"),
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn json_split_across_two_frames_is_reassembled() {
    let server = MockSse::start(Script::sse([
        raw("data: {\"choices\":[{\"index\":0,\"del\n\n"),
        raw("data: ta\":{\"content\":\"hi\"}}]}\n\n"),
        done(),
    ]))
    .await;

    let mut stream = streaming_chat_completion(&server.url, streaming_request(json!({})), None)
        .await
        .unwrap();

    let first = stream.next().await.unwrap().unwrap();
    let StreamingChatCompletionEvent::Chunk { chunk } = first else {
        panic!("expected a chunk");
    };
    assert_eq!(
        chunk.choices.unwrap()[0].delta.content.as_deref(),
        Some("hi")
    );
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        StreamingChatCompletionEvent::Done { .. }
    ));
}

#[tokio::test]
async fn unparseable_frame_yields_error_instead_of_swallowing_the_stream() {
    let server = MockSse::start(Script::sse([
        data(json!({"choices": 5})),
        data(delta(0, "hi")),
        done(),
    ]))
    .await;

    let mut stream = streaming_chat_completion(&server.url, streaming_request(json!({})), None)
        .await
        .unwrap();

    let first = stream.next().await.expect("stream should yield something");
    match first {
        Err(Error::SerdeJson(_)) => {}
        Err(other) => panic!("unexpected error variant: {other}"),
        Ok(_) => panic!("expected a deserialization error, got an event"),
    }
    assert!(
        stream.next().await.is_none(),
        "stream should end after the error"
    );
}
