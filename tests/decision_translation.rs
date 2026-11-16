use llm_shim::{
    apis::{
        decisions::models::api::{request::DecisionsRequestBody, response::DecisionsResponseBody},
        systemone::models::api::{request::SystemOneRequestBody, response::SystemOneResponseBody},
    },
    error::Error,
};
use serde_json::{Value, json};

fn decisions_request() -> DecisionsRequestBody {
    serde_json::from_value(json!({
        "model": "english",
        "input": "I was charged twice this month. Refund me or I cancel.",
        "questions": [
            {
                "type": "choice",
                "name": "department",
                "instructions": "Which department should handle this?",
                "choices": [
                    {"value": "billing", "description": "payments and refunds"},
                    {"value": "technical"}
                ]
            },
            {
                "type": "choice",
                "name": "plan",
                "instructions": "Which plan?",
                "choices": [{"value": "free"}, {"value": "pro"}]
            },
            {
                "type": "score",
                "name": "urgency",
                "instructions": "How urgent is this?",
                "levels": [{"label": "low"}, {"label": "high", "description": "needs action now"}]
            },
            {"type": "predicate", "name": "complaint", "instructions": "Is the customer complaining?"}
        ],
        "metadata": {"trace": "abc"}
    }))
    .unwrap()
}

fn to_json(value: impl serde::Serialize) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn decisions_request_becomes_systemone() {
    let body = SystemOneRequestBody::try_from(decisions_request()).unwrap();

    assert_eq!(
        to_json(&body),
        json!({
            "model": "english",
            "state": "I was charged twice this month. Refund me or I cancel.",
            "questions": {
                "department": {
                    "type": "choice",
                    "criteria": {"billing": "payments and refunds", "technical": null},
                    "instructions": "Which department should handle this?"
                },
                "plan": {
                    "type": "choice",
                    "criteria": ["free", "pro"],
                    "instructions": "Which plan?"
                },
                "urgency": {
                    "type": "score",
                    "criteria": ["low", "high: needs action now"],
                    "instructions": "How urgent is this?"
                },
                "complaint": {
                    "type": "noul",
                    "instructions": "Is the customer complaining?"
                }
            },
            "metadata": {"trace": "abc"}
        })
    );
}

#[test]
fn duplicate_question_names_are_refused() {
    let body: DecisionsRequestBody = serde_json::from_value(json!({
        "input": "x",
        "questions": [
            {"type": "predicate", "name": "q", "instructions": "a"},
            {"type": "predicate", "name": "q", "instructions": "b"}
        ]
    }))
    .unwrap();

    let result = SystemOneRequestBody::try_from(body);

    assert!(matches!(result, Err(Error::Translation(message)) if message.contains("'q'")));
}

#[test]
fn unknown_question_types_are_refused() {
    let body: DecisionsRequestBody = serde_json::from_value(json!({
        "input": "x",
        "questions": [{"type": "ranking", "name": "q"}]
    }))
    .unwrap();

    assert!(matches!(
        SystemOneRequestBody::try_from(body),
        Err(Error::Translation(_))
    ));
}

fn laya_response() -> SystemOneResponseBody {
    serde_json::from_value(json!({
        "model": "laya-rl-agent",
        "answers": {
            "complaint": {"type": "noul", "noul": 0.89, "confidence": 0.89, "action": {"act_probability": 1.0}},
            "urgency": {
                "type": "score",
                "score": 0.85,
                "legend": {"0": "low", "1": "high: needs action now"},
                "probabilities": {"0": 0.15, "1": 0.85},
                "confidence": 0.4,
                "x_jev_confidence": 0.7
            },
            "plan": {"type": "choice", "choice": "pro", "probabilities": {"free": 0.2, "pro": 0.8}},
            "department": {
                "type": "choice",
                "choice": "billing",
                "probabilities": {"technical": 0.04, "billing": 0.96},
                "confidence": 0.81,
                "answer_confidence": 0.96,
                "x_jev_confidence": 0.93
            }
        },
        "usage": {"input_tokens": 183, "output_tokens": 0, "state_tokens": 24},
        "routing": {"model": "english"}
    }))
    .unwrap()
}

#[test]
fn systemone_answer_becomes_decisions_in_question_order() {
    let request = decisions_request();

    let body = laya_response().into_decisions(&request.questions).unwrap();

    assert_eq!(
        to_json(&body),
        json!({
            "object": "decisions",
            "answers": [
                {
                    "type": "choice",
                    "name": "department",
                    "choice": "billing",
                    "confidence": 0.93,
                    "probabilities": [
                        {"value": "billing", "probability": 0.96},
                        {"value": "technical", "probability": 0.04}
                    ],
                    "answer_confidence": 0.96
                },
                {
                    "type": "choice",
                    "name": "plan",
                    "choice": "pro",
                    "probabilities": [
                        {"value": "free", "probability": 0.2},
                        {"value": "pro", "probability": 0.8}
                    ]
                },
                {
                    "type": "score",
                    "name": "urgency",
                    "score": 0.85,
                    "confidence": 0.7,
                    "probabilities": [
                        {"label": "low", "value": 0, "probability": 0.15},
                        {"label": "high", "value": 1, "probability": 0.85}
                    ]
                },
                {
                    "type": "predicate",
                    "name": "complaint",
                    "probability": 0.89,
                    "confidence": 0.89,
                    "action": {"act_probability": 1.0}
                }
            ],
            "usage": {"input_tokens": 183, "output_tokens": 0, "total_tokens": 183, "state_tokens": 24},
            "model": "laya-rl-agent",
            "routing": {"model": "english"}
        })
    );
}

#[test]
fn a_missing_answer_is_a_translation_error() {
    let mut response = laya_response();
    response.answers.as_mut().unwrap().shift_remove("urgency");

    let result = response.into_decisions(&decisions_request().questions);

    assert!(matches!(result, Err(Error::Translation(message)) if message.contains("'urgency'")));
}

#[test]
fn an_answer_of_the_wrong_type_is_a_translation_error() {
    let mut response = laya_response();
    let answers = response.answers.as_mut().unwrap();
    let complaint = answers.shift_remove("complaint").unwrap();
    answers.insert("plan".to_string(), complaint);

    let result = response.into_decisions(&decisions_request().questions);

    assert!(matches!(result, Err(Error::Translation(message)) if message.contains("'plan'")));
}

fn systemone_request() -> SystemOneRequestBody {
    serde_json::from_value(json!({
        "model": "gpt-6-luna",
        "state": {"subject": "Refund", "body": "Charged twice"},
        "questions": {
            "intent": {
                "type": "choice",
                "instructions": "What does the customer want?",
                "criteria": {"refund": "money back", "other": null}
            },
            "frustration": {"type": "score", "instructions": "How frustrated?", "criteria": ["calm", "angry"]},
            "urgent": {"type": "noul", "instructions": "Is it urgent?", "criteria": "a deadline is mentioned"}
        }
    }))
    .unwrap()
}

#[test]
fn systemone_request_becomes_decisions() {
    let body = DecisionsRequestBody::try_from(systemone_request()).unwrap();

    assert_eq!(
        to_json(&body),
        json!({
            "model": "gpt-6-luna",
            "input": "{\"subject\":\"Refund\",\"body\":\"Charged twice\"}",
            "questions": [
                {
                    "type": "choice",
                    "name": "intent",
                    "choices": [{"value": "refund", "description": "money back"}, {"value": "other"}],
                    "instructions": "What does the customer want?"
                },
                {
                    "type": "score",
                    "name": "frustration",
                    "levels": [{"label": "calm"}, {"label": "angry"}],
                    "instructions": "How frustrated?"
                },
                {
                    "type": "predicate",
                    "name": "urgent",
                    "instructions": "Is it urgent?",
                    "criteria": "a deadline is mentioned"
                }
            ]
        })
    );
}

#[test]
fn decisions_answer_becomes_systemone_in_question_order() {
    let response: DecisionsResponseBody = serde_json::from_value(json!({
        "object": "decisions",
        "model": "gpt-6-luna",
        "answers": [
            {"type": "predicate", "name": "urgent", "probability": 0.81},
            {
                "type": "score",
                "name": "frustration",
                "score": 0.6,
                "confidence": 0.34,
                "probabilities": [
                    {"label": "calm", "value": 0, "probability": 0.4},
                    {"label": "angry", "value": 1, "probability": 0.6}
                ]
            },
            {
                "type": "choice",
                "name": "intent",
                "choice": "refund",
                "probabilities": [{"value": "refund", "probability": 0.9}, {"value": "other", "probability": 0.1}]
            }
        ],
        "usage": {"input_tokens": 50, "output_tokens": 0, "total_tokens": 50}
    }))
    .unwrap();

    let body = response
        .into_systemone(&systemone_request().questions)
        .unwrap();

    let body = to_json(&body);
    assert_eq!(
        body["answers"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["intent", "frustration", "urgent"]
    );
    assert_eq!(
        body,
        json!({
            "answers": {
                "intent": {"type": "choice", "choice": "refund", "probabilities": {"refund": 0.9, "other": 0.1}},
                "frustration": {
                    "type": "score",
                    "score": 0.6,
                    "legend": {"0": "calm", "1": "angry"},
                    "probabilities": {"0": 0.4, "1": 0.6},
                    "confidence": 0.34
                },
                "urgent": {"type": "noul", "noul": 0.81}
            },
            "usage": {"input_tokens": 50, "output_tokens": 0},
            "model": "gpt-6-luna"
        })
    );
}

#[test]
fn shapes_the_typed_models_do_not_know_pass_through_unchanged() {
    let request = json!({
        "state": "x",
        "questions": {
            "rubric": {"type": "score", "criteria": [{"desc": "calm"}, {"desc": "angry"}]},
            "future": {"type": "ranking", "items": ["a", "b"]}
        }
    });
    let response = json!({
        "answers": {
            "untyped": {"choice": "happy"},
            "future": {"type": "ranking", "order": ["b", "a"]}
        }
    });

    let request_body: SystemOneRequestBody = serde_json::from_value(request.clone()).unwrap();
    let response_body: SystemOneResponseBody = serde_json::from_value(response.clone()).unwrap();

    assert_eq!(to_json(&request_body), request);
    assert_eq!(to_json(&response_body), response);
}

#[test]
fn null_noul_criteria_are_not_sent_to_a_decisions_provider() {
    let body: SystemOneRequestBody = serde_json::from_value(json!({
        "state": "hello",
        "questions": {"greeting": {"type": "noul", "instructions": "Is this a greeting?", "criteria": null}}
    }))
    .unwrap();

    let body = DecisionsRequestBody::try_from(body).unwrap();

    assert_eq!(
        to_json(&body)["questions"],
        json!([{"type": "predicate", "name": "greeting", "instructions": "Is this a greeting?"}])
    );
}
