mod support;

use llm_shim::apis::chat_completion::{
    models::lib::streaming::response::StreamingChatCompletionEvent,
    non_streaming::non_streaming_chat_completion, streaming::streaming_chat_completion,
};
use serde_json::json;
use support::*;
use tokio_stream::StreamExt;

#[tokio::test]
async fn streaming_response_can_be_moved_to_another_task() {
    let server = MockSse::start(Script::sse([data(delta(0, "hi")), done()])).await;

    let mut stream = streaming_chat_completion(&server.url, streaming_request(json!({})), None)
        .await
        .unwrap();

    let contents = tokio::spawn(async move {
        let mut contents = vec![];
        while let Some(event) = stream.next().await {
            if let StreamingChatCompletionEvent::Chunk { chunk } = event.unwrap() {
                contents.push(chunk.choices.unwrap()[0].delta.content.clone().unwrap());
            }
        }
        contents
    })
    .await
    .unwrap();

    assert_eq!(contents, ["hi"]);
}

#[tokio::test]
async fn non_streaming_future_can_be_spawned() {
    let server = MockSse::start(Script::sse([data(delta(0, "hi")), done()])).await;
    let url = server.url.clone();

    let response = tokio::spawn(async move {
        let body: llm_shim::apis::chat_completion::models::api::request::non_streaming::NonStreamingChatCompletionRequestBody =
            serde_json::from_value(json!({"model": "m", "messages": [{"role": "user", "content": "x"}]})).unwrap();
        non_streaming_chat_completion(&url, body, None).await.unwrap()
    })
    .await
    .unwrap();

    assert_eq!(
        response.body.choices[0].message.content.as_deref(),
        Some("hi")
    );
}
