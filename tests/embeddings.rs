mod support;

use async_trait::async_trait;
use llm_shim::{
    apis::embeddings::{
        embeddings,
        models::{
            api::request::{EmbeddingsInput, EmbeddingsRequestBody},
            lib::options::EmbeddingsOptions,
        },
    },
    error::Error,
    traits::max_tps::MaxTps,
};
use reqwest::StatusCode;
use serde_json::json;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use support::*;

fn request(input: serde_json::Value) -> EmbeddingsRequestBody {
    serde_json::from_value(json!({"model": "embed-model", "input": input})).unwrap()
}

fn response(vectors: &[Vec<f64>], prompt_tokens: u32) -> serde_json::Value {
    let data: Vec<_> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| json!({"object": "embedding", "index": i, "embedding": v}))
        .collect();
    json!({"object": "list", "data": data, "model": "embed-model", "usage": {"prompt_tokens": prompt_tokens, "total_tokens": prompt_tokens}})
}

struct FixedTps(f32);

#[async_trait]
impl MaxTps for FixedTps {
    async fn get(&self) -> Option<f32> {
        Some(self.0)
    }
}

struct SteppedTps(Vec<Option<f32>>, AtomicUsize);

#[async_trait]
impl MaxTps for SteppedTps {
    async fn get(&self) -> Option<f32> {
        let i = self.1.fetch_add(1, Ordering::SeqCst);
        self.0.get(i).copied().flatten()
    }
}

struct CountingTps<'a>(&'a dyn MaxTps, AtomicUsize);

#[async_trait]
impl MaxTps for CountingTps<'_> {
    async fn get(&self) -> Option<f32> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.get().await
    }
}

struct SlowTps(Duration, AtomicUsize);

#[async_trait]
impl MaxTps for SlowTps {
    async fn get(&self) -> Option<f32> {
        self.1.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.0).await;
        Some(1000.0)
    }
}

#[tokio::test]
async fn parses_vectors_and_reports_exact_input_tokens() {
    let server = MockSse::start(Script::json(response(&[vec![0.1, 0.2], vec![0.3, 0.4]], 7))).await;

    let result = embeddings(&server.embeddings_url, request(json!(["a", "b"])), None)
        .await
        .unwrap();

    let data = result.body.additional_properties["data"]
        .as_array()
        .unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[1]["index"], json!(1));
    assert_eq!(data[1]["embedding"], json!([0.3, 0.4]));
    assert_eq!(
        result.body.additional_properties["model"],
        json!("embed-model")
    );
    assert_eq!(result.body.usage.as_ref().unwrap().prompt_tokens, Some(7));
    assert_eq!(result.body.additional_properties["object"], json!("list"));

    assert!(!result.stats.input_tokens_is_estimate);
    assert_eq!(result.stats.input_tokens, 7);
    assert_eq!(result.stats.output_tokens, 0);
    assert!(!result.stats.output_tokens_is_estimate);
    assert_eq!(result.stats.chunks.len(), 1);
    assert_eq!(result.stats.chunks[0].tokens, 7);
    assert!(result.stats.chunks[0].tps_correction_duration.is_none());
}

#[tokio::test]
async fn sends_bearer_token_and_passes_extra_fields_through() {
    let server = MockSse::start(Script::json(response(&[vec![0.0]], 1))).await;

    let body: EmbeddingsRequestBody = serde_json::from_value(json!({
        "model": "embed-model", "input": "x", "dimensions": 256, "encoding_format": "float", "user": "u1"
    }))
    .unwrap();
    assert_eq!(body.additional_properties["dimensions"], json!(256));

    embeddings(
        &server.embeddings_url,
        body,
        EmbeddingsOptions::default().with_bearer_token("tok"),
    )
    .await
    .unwrap();

    assert_eq!(
        server.single_request_headers()["authorization"],
        "Bearer tok"
    );
    let sent = server.single_request();
    assert_eq!(sent["dimensions"], json!(256));
    assert_eq!(sent["user"], json!("u1"));
    assert_eq!(sent["input"], json!("x"));
}

#[tokio::test]
async fn accepts_all_input_shapes_and_base64_output() {
    let server = MockSse::start(Script::json(json!({
        "object": "list", "model": "m",
        "data": [{"object": "embedding", "index": 0, "embedding": "AAAAAA=="}],
        "usage": {"prompt_tokens": 3, "total_tokens": 3}
    })))
    .await;

    assert!(matches!(
        request(json!("a")).input,
        EmbeddingsInput::Text(_)
    ));
    assert!(matches!(
        request(json!(["a", "b"])).input,
        EmbeddingsInput::Texts(_)
    ));
    assert!(matches!(
        request(json!([1, 2, 3])).input,
        EmbeddingsInput::Tokens(_)
    ));
    assert!(matches!(
        request(json!([[1, 2], [3]])).input,
        EmbeddingsInput::TokenBatches(_)
    ));

    let result = embeddings(&server.embeddings_url, request(json!([1, 2, 3])), None)
        .await
        .unwrap();

    assert_eq!(
        result.body.additional_properties["data"][0]["embedding"],
        json!("AAAAAA==")
    );
}

#[tokio::test]
async fn estimates_input_tokens_when_usage_is_missing() {
    let server = MockSse::start(Script::json(json!({"object": "list", "data": [{"object": "embedding", "index": 0, "embedding": [0.0]}]}))).await;

    let result = embeddings(
        &server.embeddings_url,
        request(json!("hello there world")),
        None,
    )
    .await
    .unwrap();

    assert!(result.stats.input_tokens_is_estimate);
    assert!(result.stats.input_tokens > 0);
    assert_eq!(result.stats.chunks[0].tokens, result.stats.input_tokens);

    let server = MockSse::start(Script::json(json!({"object": "list", "data": []}))).await;
    let result = embeddings(
        &server.embeddings_url,
        request(json!([[1, 2], [3, 4, 5]])),
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.stats.input_tokens, 5);
}

#[tokio::test]
async fn non_2xx_is_an_error() {
    let server = MockSse::start(Script::error(
        StatusCode::UNAUTHORIZED,
        json!({"error": {"message": "bad key"}}),
    ))
    .await;

    match embeddings(&server.embeddings_url, request(json!("x")), None).await {
        Err(Error::Reqwest(e)) => assert_eq!(e.status(), Some(StatusCode::UNAUTHORIZED)),
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn sleeps_before_returning_when_the_request_exceeded_max_tps() {
    let server = MockSse::start(Script::json(response(&[vec![0.0]], 50))).await;
    let throttler = FixedTps(100.0);

    let started = Instant::now();
    let result = embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();
    let elapsed = started.elapsed();

    assert!(elapsed >= Duration::from_millis(450), "elapsed {elapsed:?}");
    let chunk = result.stats.chunks[0];
    assert!(chunk.tps_correction_duration.is_some());
    assert!(result.stats.tps_avg().unwrap() <= 101.0);
}

#[tokio::test]
async fn does_not_sleep_when_the_request_was_already_slow_enough() {
    let server =
        MockSse::start(Script::json(response(&[vec![0.0]], 5)).delayed(Duration::from_millis(100)))
            .await;
    let throttler = FixedTps(1000.0);

    let result = embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();

    assert!(result.stats.chunks[0].tps_correction_duration.is_none());
    assert!(result.stats.chunks[0].duration >= Duration::from_millis(100));
}

#[tokio::test]
async fn reads_max_tps_once_when_the_request_finishes_before_the_first_tick() {
    let server = MockSse::start(Script::json(response(&[vec![0.0]], 50))).await;
    let throttler = SteppedTps(vec![Some(100.0), Some(1000.0)], AtomicUsize::new(0));

    let started = Instant::now();
    embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();

    assert!(
        started.elapsed() >= Duration::from_millis(450),
        "elapsed {:?}",
        started.elapsed()
    );
    assert_eq!(throttler.1.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn samples_max_tps_on_the_interval_while_the_request_is_in_flight() {
    let server = MockSse::start(
        Script::json(response(&[vec![0.0]], 50)).delayed(Duration::from_millis(600)),
    )
    .await;
    let throttler = SteppedTps(
        vec![
            Some(50.0),
            Some(150.0),
            Some(150.0),
            Some(150.0),
            Some(150.0),
        ],
        AtomicUsize::new(0),
    );

    let started = Instant::now();
    embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();
    let elapsed = started.elapsed();

    let reads = throttler.1.load(Ordering::SeqCst);
    assert!((3..=8).contains(&reads), "reads {reads}");
    assert!(elapsed < Duration::from_millis(950), "elapsed {elapsed:?}");
}

#[tokio::test]
async fn default_sample_interval_is_250_ms() {
    assert_eq!(
        EmbeddingsOptions::default().max_tps_sample_interval,
        Some(llm_shim::tps_throttler::DEFAULT_MAX_TPS_SAMPLE_INTERVAL)
    );
    assert_eq!(
        EmbeddingsOptions::default().max_tps_sample_interval,
        Some(Duration::from_millis(250))
    );
}

#[tokio::test]
async fn no_sample_interval_reads_max_tps_exactly_once() {
    let server =
        MockSse::start(Script::json(response(&[vec![0.0]], 5)).delayed(Duration::from_millis(300)))
            .await;
    let throttler = SteppedTps(
        vec![Some(1000.0), Some(1000.0), Some(1000.0)],
        AtomicUsize::new(0),
    );

    embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default()
            .with_max_tps(&throttler)
            .without_max_tps_sample_interval(),
    )
    .await
    .unwrap();

    assert_eq!(throttler.1.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn zero_sample_interval_reads_max_tps_back_to_back() {
    let server =
        MockSse::start(Script::json(response(&[vec![0.0]], 5)).delayed(Duration::from_millis(100)))
            .await;
    let throttler = FixedTps(1000.0);
    let counter = CountingTps(&throttler, AtomicUsize::new(0));

    let result = embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default()
            .with_max_tps(&counter)
            .with_max_tps_sample_interval(Duration::ZERO),
    )
    .await
    .unwrap();

    assert_eq!(
        result.body.additional_properties["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let reads = counter.1.load(Ordering::SeqCst);
    assert!(reads > 100, "reads {reads}");
}

#[tokio::test]
async fn short_sample_interval_reads_max_tps_on_every_tick() {
    let server =
        MockSse::start(Script::json(response(&[vec![0.0]], 5)).delayed(Duration::from_millis(300)))
            .await;
    let throttler = FixedTps(1000.0);
    let counter = CountingTps(&throttler, AtomicUsize::new(0));

    embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default()
            .with_max_tps(&counter)
            .with_max_tps_sample_interval(Duration::from_millis(50)),
    )
    .await
    .unwrap();

    let reads = counter.1.load(Ordering::SeqCst);
    assert!((5..=12).contains(&reads), "reads {reads}");
}

#[tokio::test]
async fn a_slow_max_tps_source_is_not_polled_in_a_burst() {
    let server =
        MockSse::start(Script::json(response(&[vec![0.0]], 5)).delayed(Duration::from_millis(400)))
            .await;
    let throttler = SlowTps(Duration::from_millis(100), AtomicUsize::new(0));

    embeddings(
        &server.embeddings_url,
        request(json!("x")),
        EmbeddingsOptions::default()
            .with_max_tps(&throttler)
            .with_max_tps_sample_interval(Duration::from_millis(20)),
    )
    .await
    .unwrap();

    let reads = throttler.1.load(Ordering::SeqCst);
    assert!((2..=6).contains(&reads), "reads {reads}");
}
