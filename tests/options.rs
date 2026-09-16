mod support;

use async_trait::async_trait;
use llm_shim::{
    apis::chat_completion::{
        chat_completion,
        models::{
            api::response::ChatCompletionResponse,
            lib::options::{ChatCompletionOptions, OutputTokenCounting},
        },
        streaming::streaming_chat_completion,
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
async fn top_level_chat_completion_accepts_a_borrowed_throttler() {
    let server = MockSse::start(Script::sse([data(delta(0, "hi")), done()])).await;
    let throttler = FixedTps(1000.0);
    let options = ChatCompletionOptions::default().with_max_tps(&throttler);

    let response = chat_completion(&server.url, streaming_request(json!({})), options)
        .await
        .unwrap();

    let ChatCompletionResponse::Streaming(mut stream) = response else {
        panic!("expected a streaming response");
    };
    let mut count = 0;
    while let Some(event) = stream.next().await {
        event.unwrap();
        count += 1;
    }
    assert_eq!(count, 2);
}

#[tokio::test]
async fn logprobs_token_counting_is_on_by_default() {
    let server = MockSse::start(Script::sse([done()])).await;

    let options = ChatCompletionOptions::default();
    assert_eq!(options.output_token_counting, OutputTokenCounting::Logprobs);

    let mut stream = streaming_chat_completion(&server.url, streaming_request(json!({})), options)
        .await
        .unwrap();
    while stream.next().await.is_some() {}

    assert_eq!(server.single_request()["logprobs"], json!(true));
}

#[tokio::test]
async fn estimate_counting_leaves_logprobs_untouched_in_the_request() {
    let server = MockSse::start(Script::sse([data(delta(0, "hello there")), done()])).await;

    let options =
        ChatCompletionOptions::default().with_output_token_counting(OutputTokenCounting::Estimate);

    let mut stream = streaming_chat_completion(&server.url, streaming_request(json!({})), options)
        .await
        .unwrap();
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    let sent = server.single_request();
    assert!(sent.get("logprobs").is_none(), "sent: {sent}");
    assert_eq!(sent["stream_options"]["include_usage"], json!(true));

    let llm_shim::apis::chat_completion::models::lib::streaming::response::StreamingChatCompletionEvent::Done { stats } =
        events.last().unwrap()
    else {
        panic!("expected Done");
    };
    assert!(stats.output_tokens_is_estimate);
    assert!(stats.output_tokens > 0);
}

#[tokio::test]
async fn estimate_counting_still_forwards_logprobs_the_caller_asked_for() {
    let server = MockSse::start(Script::sse([done()])).await;

    let options =
        ChatCompletionOptions::default().with_output_token_counting(OutputTokenCounting::Estimate);

    let mut stream = streaming_chat_completion(
        &server.url,
        streaming_request(json!({"logprobs": true})),
        options,
    )
    .await
    .unwrap();
    while stream.next().await.is_some() {}

    assert_eq!(server.single_request()["logprobs"], json!(true));
}
