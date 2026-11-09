mod support;

use llm_shim::{
    apis::embeddings::{
        embeddings,
        models::{api::request::EmbeddingsRequestBody, lib::options::EmbeddingsOptions},
    },
    credentials::Credentials,
    error::Error,
};
use serde_json::json;
use support::*;

fn request() -> EmbeddingsRequestBody {
    serde_json::from_value(json!({"model": "m", "input": "hi"})).unwrap()
}

async fn sent_headers(credentials: Option<Credentials>) -> reqwest::header::HeaderMap {
    let server = MockSse::start(Script::json(json!({"data": []}))).await;
    let mut options = EmbeddingsOptions::default();
    if let Some(credentials) = credentials {
        options = options.with_credentials(credentials);
    }
    embeddings(&server.embeddings_url, request(), options)
        .await
        .unwrap();
    server.single_request_headers()
}

#[tokio::test]
async fn bearer_sets_authorization() {
    let headers = sent_headers(Some(Credentials::bearer("tok"))).await;
    assert_eq!(headers["authorization"], "Bearer tok");
}

#[tokio::test]
async fn basic_sets_authorization_with_and_without_password() {
    let headers = sent_headers(Some(Credentials::basic("user", Some("pass")))).await;
    // base64("user:pass")
    assert_eq!(headers["authorization"], "Basic dXNlcjpwYXNz");

    let headers = sent_headers(Some(Credentials::basic("user", None::<&str>))).await;
    // base64("user:")
    assert_eq!(headers["authorization"], "Basic dXNlcjo=");
}

#[tokio::test]
async fn x_api_key_sets_header_without_authorization() {
    let headers = sent_headers(Some(Credentials::x_api_key("sk-test"))).await;
    assert_eq!(headers["x-api-key"], "sk-test");
    assert!(headers.get("authorization").is_none());
}

#[tokio::test]
async fn arbitrary_header_is_sent() {
    let headers = sent_headers(Some(Credentials::header("api-key", "azure-key"))).await;
    assert_eq!(headers["api-key"], "azure-key");
    assert!(headers.get("authorization").is_none());
}

#[tokio::test]
async fn no_credentials_sends_no_auth_headers() {
    let headers = sent_headers(None).await;
    assert!(headers.get("authorization").is_none());
    assert!(headers.get("x-api-key").is_none());
}

#[tokio::test]
async fn invalid_header_name_errors_instead_of_being_dropped() {
    let server = MockSse::start(Script::json(json!({"data": []}))).await;
    let result = embeddings(
        &server.embeddings_url,
        request(),
        EmbeddingsOptions::default().with_credentials(Credentials::header("bad header", "v")),
    )
    .await;

    assert!(matches!(result, Err(Error::Reqwest(_))));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn invalid_header_value_errors_instead_of_being_dropped() {
    let server = MockSse::start(Script::json(json!({"data": []}))).await;
    let result = embeddings(
        &server.embeddings_url,
        request(),
        EmbeddingsOptions::default().with_credentials(Credentials::x_api_key("bad\nkey")),
    )
    .await;

    assert!(matches!(result, Err(Error::Reqwest(_))));
    assert!(server.requests().is_empty());
}

#[test]
fn debug_output_redacts_secrets() {
    let debug = format!(
        "{:?} {:?} {:?} {:?}",
        Credentials::bearer("s3cret-1"),
        Credentials::basic("user", Some("s3cret-2")),
        Credentials::x_api_key("s3cret-3"),
        Credentials::header("api-key", "s3cret-4"),
    );
    assert!(!debug.contains("s3cret"), "{debug}");
    assert!(debug.contains("user") && debug.contains("api-key"));
}
