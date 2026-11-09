mod support;

use llm_shim::credentials::Credentials;

use async_trait::async_trait;
use llm_shim::{
    apis::decisions::{
        decisions,
        models::{api::request::DecisionsRequestBody, lib::options::DecisionsOptions},
    },
    error::Error,
    traits::max_tps::MaxTps,
};
use reqwest::StatusCode;
use serde_json::json;
use std::time::{Duration, Instant};
use support::*;

fn request(extra: serde_json::Value) -> DecisionsRequestBody {
    let mut body = json!({
        "input": [
            {"role": "developer", "content": "You triage support tickets."},
            {"role": "user", "content": [
                {"type": "input_text", "text": "I was charged twice this month. Refund me or I cancel."}
            ]}
        ],
        "questions": [
            {
                "type": "choice",
                "name": "intent",
                "instructions": "What does the customer want?",
                "choices": [
                    {"value": "refund", "description": "money back or a duplicate charge reversed"},
                    {"value": "technical_help"},
                    {"value": "cancellation"},
                    {"value": "other"}
                ]
            },
            {"type": "predicate", "name": "urgent", "instructions": "Does the message communicate time pressure?"},
            {
                "type": "score",
                "name": "frustration",
                "instructions": "How frustrated does the customer sound?",
                "levels": [{"label": "calm"}, {"label": "concerned"}, {"label": "annoyed"}, {"label": "very angry"}]
            }
        ]
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

fn response(input_tokens: u32) -> serde_json::Value {
    json!({
        "model": "gpt-6-luna",
        "answers": [
            {
                "type": "choice",
                "name": "intent",
                "choice": "refund",
                "confidence": 0.79,
                "probabilities": [
                    {"value": "refund", "probability": 0.9466},
                    {"value": "technical_help", "probability": 0.0071},
                    {"value": "cancellation", "probability": 0.0385},
                    {"value": "other", "probability": 0.0078}
                ]
            },
            {"type": "predicate", "name": "urgent", "probability": 0.81},
            {
                "type": "score",
                "name": "frustration",
                "score": 2.31,
                "confidence": 0.34,
                "probabilities": [
                    {"label": "calm", "value": 0, "probability": 0.03},
                    {"label": "concerned", "value": 1, "probability": 0.12},
                    {"label": "annoyed", "value": 2, "probability": 0.36},
                    {"label": "very angry", "value": 3, "probability": 0.49}
                ]
            }
        ],
        "usage": {
            "input_tokens": input_tokens,
            "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
            "output_tokens": 0,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": input_tokens
        }
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
async fn parses_answers_and_reports_tokens_from_usage() {
    let server = MockSse::start(Script::json(response(212))).await;

    let result = decisions(
        &server.decisions_url,
        request(json!({"model": "gpt-6-luna"})),
        DecisionsOptions::default().with_credentials(Credentials::bearer("sk-x")),
    )
    .await
    .unwrap();

    let answers = &result.body.additional_properties["answers"];
    assert_eq!(answers[0]["choice"], json!("refund"));
    assert_eq!(answers[1]["probability"], json!(0.81));
    assert_eq!(answers[2]["probabilities"][3]["label"], json!("very angry"));
    assert_eq!(
        result.body.additional_properties["model"],
        json!("gpt-6-luna")
    );

    let usage = result.body.usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, Some(212));
    assert_eq!(usage.output_tokens, Some(0));
    assert_eq!(usage.additional_properties["total_tokens"], json!(212));
    assert_eq!(
        usage.additional_properties["input_tokens_details"]["cached_tokens"],
        json!(0)
    );

    assert!(!result.stats.input_tokens_is_estimate);
    assert!(!result.stats.output_tokens_is_estimate);
    assert_eq!(result.stats.input_tokens, 212);
    assert_eq!(result.stats.chunks[0].tokens, 212);

    let sent = server.single_request();
    assert_eq!(sent["model"], json!("gpt-6-luna"));
    // Question order is preserved.
    assert_eq!(
        sent["questions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| q["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["intent", "urgent", "frustration"]
    );
    assert_eq!(sent["input"][1]["content"][0]["type"], json!("input_text"));
    assert_eq!(
        server.single_request_headers()["authorization"],
        "Bearer sk-x"
    );
}

#[tokio::test]
async fn estimates_input_once_per_question_when_usage_is_missing() {
    let server = MockSse::start(Script::json(json!({"answers": []}))).await;

    let one = request(json!({"questions": [{"type": "predicate", "instructions": "x"}]}));
    let two = request(json!({"questions": [
        {"type": "predicate", "instructions": "x"},
        {"type": "predicate", "instructions": "x"}
    ]}));

    let result = decisions(&server.decisions_url, two, None).await.unwrap();

    assert!(result.stats.input_tokens_is_estimate);
    assert!(result.body.usage.is_none());
    use llm_shim::traits::estimate_tokens::EstimateTokens;
    assert_eq!(result.stats.input_tokens, 2 * one.estimate_tokens());
}

#[tokio::test]
async fn plain_string_input_and_unknown_fields_pass_through() {
    let server = MockSse::start(Script::json(response(3))).await;

    let body = request(json!({
        "input": "just text",
        "prompt_cache_key": "tickets",
        "safety_identifier": "user-123"
    }));
    decisions(&server.decisions_url, body, None).await.unwrap();

    let sent = server.single_request();
    assert_eq!(sent["input"], json!("just text"));
    assert_eq!(sent["prompt_cache_key"], json!("tickets"));
    assert_eq!(sent["safety_identifier"], json!("user-123"));
    assert!(sent.get("model").is_none());
}

#[tokio::test]
async fn refusal_answers_pass_through() {
    let server = MockSse::start(Script::json(json!({
        "model": "gpt-6-luna",
        "answers": [{"type": "refusal", "name": null}],
        "usage": {"input_tokens": 9, "output_tokens": 0, "total_tokens": 9}
    })))
    .await;

    let result = decisions(&server.decisions_url, request(json!({})), None)
        .await
        .unwrap();

    let answers = &result.body.additional_properties["answers"];
    assert_eq!(answers[0]["type"], json!("refusal"));
    assert!(answers[0]["name"].is_null());
    assert_eq!(result.stats.input_tokens, 9);
}

#[tokio::test]
async fn bad_request_surfaces_400() {
    let server = MockSse::start(Script::error(
        StatusCode::BAD_REQUEST,
        json!({"error": {"message": "choices must be unique", "type": "invalid_request_error"}}),
    ))
    .await;

    match decisions(&server.decisions_url, request(json!({})), None).await {
        Err(Error::Reqwest(error)) => {
            assert_eq!(error.status(), Some(StatusCode::BAD_REQUEST))
        }
        Err(other) => panic!("unexpected error variant: {other}"),
        Ok(_) => panic!("expected an error"),
    }
}

#[tokio::test]
async fn sleeps_before_returning_when_the_request_exceeded_max_tps() {
    let server = MockSse::start(Script::json(response(50))).await;
    let throttler = FixedTps(100.0);

    let started = Instant::now();
    let result = decisions(
        &server.decisions_url,
        request(json!({})),
        DecisionsOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(450));
    assert!(result.stats.chunks[0].tps_correction_duration.is_some());
}
