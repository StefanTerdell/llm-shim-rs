mod support;

use async_trait::async_trait;
use llm_stream_map::{
    rerank::{
        models::{api::request::RerankRequestBody, lib::options::RerankOptions},
        rerank,
    },
    traits::max_tps::MaxTps,
};
use serde_json::json;
use std::time::{Duration, Instant};
use support::*;

fn request(extra: serde_json::Value) -> RerankRequestBody {
    let mut body = json!({"model": "rerank-model", "query": "best pizza", "documents": ["pasta", "pizza napoli", "sushi"]});
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

fn response(total_tokens: u32) -> serde_json::Value {
    json!({
        "model": "rerank-model",
        "usage": {"total_tokens": total_tokens},
        "results": [
            {"index": 1, "relevance_score": 0.91, "document": {"text": "pizza napoli"}},
            {"index": 0, "relevance_score": 0.12, "document": {"text": "pasta"}}
        ]
    })
}

struct FixedTps(f32);

#[async_trait]
impl MaxTps for FixedTps {
    async fn get(&self) -> Option<f32> {
        Some(self.0)
    }
}

#[tokio::test]
async fn parses_results_and_reports_tokens_from_usage() {
    let server = MockSse::start(Script::json(response(42))).await;

    let result = rerank(
        &server.rerank_url,
        request(json!({"top_n": 2, "return_documents": true})),
        RerankOptions::default().with_bearer_token("jina-key"),
    )
    .await
    .unwrap();

    assert_eq!(result.body.results.len(), 2);
    assert_eq!(result.body.results[0].index, 1);
    assert!((result.body.results[0].relevance_score - 0.91).abs() < 1e-9);
    assert_eq!(
        result.body.results[0].document.as_ref().unwrap()["text"],
        json!("pizza napoli")
    );
    assert_eq!(result.body.usage.as_ref().unwrap().total_tokens, Some(42));

    assert!(!result.stats.input_tokens_is_estimate);
    assert_eq!(result.stats.input_tokens, 42);
    assert_eq!(result.stats.chunks[0].tokens, 42);

    let sent = server.single_request();
    assert_eq!(sent["top_n"], json!(2));
    assert_eq!(sent["return_documents"], json!(true));
    assert_eq!(sent["documents"].as_array().unwrap().len(), 3);
    assert_eq!(
        server.single_request_headers()["authorization"],
        "Bearer jina-key"
    );
}

#[tokio::test]
async fn estimates_tokens_from_query_and_documents_when_usage_is_missing() {
    let server = MockSse::start(Script::json(json!({"results": []}))).await;

    let result = rerank(&server.rerank_url, request(json!({})), None)
        .await
        .unwrap();

    assert!(result.stats.input_tokens_is_estimate);
    assert!(result.stats.input_tokens > 0);
    assert!(result.body.results.is_empty());
    assert!(result.body.usage.is_none());
}

#[tokio::test]
async fn object_documents_pass_through() {
    let server = MockSse::start(Script::json(response(3))).await;

    let body = request(json!({"documents": [{"text": "a"}, {"image": "https://x/y.png"}]}));
    rerank(&server.rerank_url, body, None).await.unwrap();

    assert_eq!(
        server.single_request()["documents"][1]["image"],
        json!("https://x/y.png")
    );
}

#[tokio::test]
async fn sleeps_before_returning_when_the_request_exceeded_max_tps() {
    let server = MockSse::start(Script::json(response(50))).await;
    let throttler = FixedTps(100.0);

    let started = Instant::now();
    let result = rerank(
        &server.rerank_url,
        request(json!({})),
        RerankOptions::default().with_tps_throttler(&throttler),
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(450));
    assert!(result.stats.chunks[0].tps_correction_duration.is_some());
}
