mod support;

use async_trait::async_trait;
use llm_shim::{
    apis::{
        chat_completion::{
            models::lib::streaming::response::StreamingChatCompletionEvent,
            streaming::streaming_chat_completion,
            virtual_non_streaming::virtual_non_streaming_chat_completion,
        },
        embeddings::{embeddings, models::lib::options::EmbeddingsOptions},
    },
    traits::max_tps::MaxTps,
};
use serde_json::json;
use support::*;
use tokio_stream::StreamExt;

struct FixedTps(f32);

#[async_trait]
impl MaxTps for FixedTps {
    async fn get(&self) -> Option<f32> {
        Some(self.0)
    }
}

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
        virtual_non_streaming_chat_completion(&url, body, None).await.unwrap()
    })
    .await
    .unwrap();

    assert_eq!(
        response.body.choices[0].message.content.as_deref(),
        Some("hi")
    );
}

#[tokio::test]
async fn throttled_one_shot_future_can_be_spawned() {
    let server = MockSse::start(Script::json(json!({
        "object": "list",
        "data": [{"object": "embedding", "index": 0, "embedding": [0.0]}],
        "model": "m",
        "usage": {"prompt_tokens": 1, "total_tokens": 1}
    })))
    .await;
    let url = server.embeddings_url.clone();

    let result = tokio::spawn(async move {
        let throttler = FixedTps(1000.0);
        let body: llm_shim::apis::embeddings::models::api::request::EmbeddingsRequestBody =
            serde_json::from_value(json!({"model": "embed-model", "input": "x"})).unwrap();

        embeddings(
            &url,
            body,
            EmbeddingsOptions::default().with_max_tps(&throttler),
        )
        .await
        .unwrap()
    })
    .await
    .unwrap();

    let data = result.body.additional_properties["data"]
        .as_array()
        .unwrap();
    assert_eq!(data.len(), 1);
}
