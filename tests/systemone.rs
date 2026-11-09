mod support;

use llm_shim::credentials::Credentials;

use async_trait::async_trait;
use llm_shim::{
    apis::systemone::{
        models::{api::request::SystemOneRequestBody, lib::options::SystemOneOptions},
        systemone,
    },
    error::Error,
    traits::max_tps::MaxTps,
};
use reqwest::StatusCode;
use serde_json::json;
use std::time::{Duration, Instant};
use support::*;

fn request(extra: serde_json::Value) -> SystemOneRequestBody {
    let mut body = json!({
        "state": {"message": "I was charged twice this month. Refund me or I cancel."},
        "questions": {
            "intent": {
                "type": "choice",
                "instructions": "What does the customer want?",
                "criteria": {
                    "refund": "money back or a duplicate charge reversed",
                    "technical_help": "a bug or integration problem",
                    "cancellation": "wants to cancel",
                    "other": "anything else"
                }
            },
            "urgent": {"type": "noul", "instructions": "Does the message communicate time pressure?"},
            "frustration": {
                "type": "score",
                "instructions": "How frustrated does the customer sound?",
                "criteria": ["calm", "concerned", "annoyed", "very angry"]
            }
        }
    });
    body.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    serde_json::from_value(body).unwrap()
}

fn response(input_tokens: u32) -> serde_json::Value {
    json!({
        "id": "dec_6f0c",
        "model": "laya-rl-agent",
        "answers": {
            "intent": {
                "type": "choice",
                "choice": "refund",
                "probabilities": {"refund": 0.9466, "technical_help": 0.0071, "cancellation": 0.0385, "other": 0.0078},
                "confidence": 0.7899,
                "action": {"act_probability": 1.0}
            },
            "urgent": {"type": "noul", "noul": 0.81, "confidence": 0.81, "action": {"act_probability": 1.0}},
            "frustration": {
                "type": "score", "score": 2.31,
                "legend": {"0": "calm", "1": "concerned", "2": "annoyed", "3": "very angry"},
                "probabilities": {"0": 0.03, "1": 0.12, "2": 0.36, "3": 0.49},
                "confidence": 0.34, "action": {"act_probability": 1.0}
            }
        },
        "usage": {"input_tokens": input_tokens, "output_tokens": 0},
        "routing": {"model": "english", "repo": "convaiinnovations/laya", "reason": "English Latin text"}
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

    let result = systemone(
        &server.systemone_url,
        request(json!({"model": "english"})),
        SystemOneOptions::default().with_credentials(Credentials::bearer("lsk_live_x")),
    )
    .await
    .unwrap();

    let answers = &result.body.additional_properties["answers"];
    assert_eq!(answers["intent"]["choice"], json!("refund"));
    assert_eq!(answers["urgent"]["noul"], json!(0.81));
    assert_eq!(answers["frustration"]["legend"]["3"], json!("very angry"));
    assert_eq!(
        result.body.additional_properties["routing"]["model"],
        json!("english")
    );

    let usage = result.body.usage.as_ref().unwrap();
    assert_eq!(usage.input_tokens, Some(212));
    assert_eq!(usage.output_tokens, Some(0));

    assert!(!result.stats.input_tokens_is_estimate);
    assert!(!result.stats.output_tokens_is_estimate);
    assert_eq!(result.stats.input_tokens, 212);
    assert_eq!(result.stats.chunks[0].tokens, 212);

    let sent = server.single_request();
    assert_eq!(sent["model"], json!("english"));
    assert_eq!(sent["questions"].as_object().unwrap().len(), 3);
    // Question order is preserved.
    assert_eq!(
        sent["questions"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["intent", "urgent", "frustration"]
    );
    assert_eq!(
        server.single_request_headers()["authorization"],
        "Bearer lsk_live_x"
    );
}

#[tokio::test]
async fn estimates_state_once_per_question_when_usage_is_missing() {
    let server = MockSse::start(Script::json(json!({"answers": {}}))).await;

    let one = request(json!({"questions": {"q": {"type": "noul", "instructions": "x"}}}));
    let two = request(
        json!({"questions": {"q": {"type": "noul", "instructions": "x"}, "r": {"type": "noul", "instructions": "x"}}}),
    );

    let result = systemone(&server.systemone_url, two, None).await.unwrap();

    assert!(result.stats.input_tokens_is_estimate);
    assert!(result.body.usage.is_none());
    use llm_shim::traits::estimate_tokens::EstimateTokens;
    assert_eq!(result.stats.input_tokens, 2 * one.estimate_tokens());
}

#[tokio::test]
async fn plain_string_state_and_unknown_fields_pass_through() {
    let server = MockSse::start(Script::json(response(3))).await;

    let body = request(json!({"state": "just text", "metadata": {"trace": "abc"}}));
    systemone(&server.systemone_url, body, None).await.unwrap();

    let sent = server.single_request();
    assert_eq!(sent["state"], json!("just text"));
    assert_eq!(sent["metadata"]["trace"], json!("abc"));
    assert!(sent.get("model").is_none());
}

#[tokio::test]
async fn malformed_question_surfaces_422() {
    let server = MockSse::start(Script::error(
        StatusCode::UNPROCESSABLE_ENTITY,
        json!({"error": "question 'frustration': a score needs a list"}),
    ))
    .await;

    match systemone(&server.systemone_url, request(json!({})), None).await {
        Err(Error::Reqwest(error)) => {
            assert_eq!(error.status(), Some(StatusCode::UNPROCESSABLE_ENTITY))
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
    let result = systemone(
        &server.systemone_url,
        request(json!({})),
        SystemOneOptions::default().with_max_tps(&throttler),
    )
    .await
    .unwrap();

    assert!(started.elapsed() >= Duration::from_millis(450));
    assert!(result.stats.chunks[0].tps_correction_duration.is_some());
}
