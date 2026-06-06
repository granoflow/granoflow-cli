use assert_cmd::Command;
use granoflow::openapi_drift::{
    CLI_KNOWN_PATHS, CRITICAL_OPENAPI_PATHS, INTENTIONALLY_UNSUPPORTED_OPENAPI_PATHS,
};
use predicates::prelude::*;
use serde_json::Value;
use tempfile::NamedTempFile;
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[test]
fn help_json_reports_language_fallback_and_known_paths() {
    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json", "--lang", "zh-HK", "help", "task", "create", "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["requestedLang"], "zh-HK");
    assert_eq!(envelope["data"]["resolvedLang"], "zh-TW");
    assert!(envelope["data"]["cliKnownPaths"]
        .as_array()
        .unwrap()
        .iter()
        .any(|path| path == "/v1/capabilities"));
}

#[test]
fn config_json_redacts_token() {
    Command::cargo_bin("granoflow")
        .unwrap()
        .args(["--json", "--token", "abcdef123456", "config"])
        .assert()
        .success()
        .stdout(predicate::str::contains("abc").and(predicate::str::contains("456")))
        .stdout(predicate::str::contains("abcdef123456").not());
}

#[test]
fn config_precedence_is_flags_env_config_defaults() {
    let config = NamedTempFile::new().unwrap();
    std::fs::write(
        config.path(),
        r#"api_base_url = "http://config.example"
token = "config-token"
"#,
    )
    .unwrap();

    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .env("GRANOFLOW_API_BASE_URL", "http://env.example")
        .env("GRANOFLOW_API_TOKEN", "env-token")
        .args([
            "--json",
            "--config",
            config.path().to_str().unwrap(),
            "--api-base-url",
            "http://flag.example",
            "--token",
            "flag-token",
            "config",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["data"]["apiBaseUrl"], "http://flag.example");
    assert_eq!(envelope["data"]["apiBaseUrlSource"], "flag");
    assert_eq!(envelope["data"]["tokenSource"], "flag");
    assert!(!envelope.to_string().contains("flag-token"));
}

#[test]
fn task_create_dry_run_does_not_call_api() {
    let input = NamedTempFile::new().unwrap();
    std::fs::write(input.path(), r#"{"title":"Draft"}"#).unwrap();
    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "task",
            "create",
            "--input",
            input.path().to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["data"]["previewMode"], "local_request_only");
    assert_eq!(envelope["data"]["method"], "POST");
    assert_eq!(envelope["data"]["path"], "/v1/tasks");
}

#[test]
fn task_complete_uses_business_complete_route_in_dry_run() {
    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args(["--json", "task", "complete", "--id", "task-1", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["data"]["previewMode"], "local_request_only");
    assert_eq!(envelope["data"]["method"], "POST");
    assert_eq!(envelope["data"]["path"], "/v1/tasks/task-1/complete");
    assert_eq!(envelope["data"]["body"], serde_json::json!({}));
}

#[test]
fn reads_json_input_from_stdin() {
    let mut command = Command::cargo_bin("granoflow").unwrap();
    command
        .args(["--json", "project", "create", "--input", "-", "--dry-run"])
        .write_stdin(r#"{"title":"From stdin"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"title\": \"From stdin\""));
}

#[tokio::test]
async fn health_calls_configured_api_base_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "running": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args(["--json", "--api-base-url", &server.uri(), "health"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\": \"ok\""));
}

#[tokio::test]
async fn deck_list_calls_review_card_decks_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/review-card-decks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"entity_type": "review-card-deck", "items": []}
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args(["--json", "--api-base-url", &server.uri(), "deck", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("review-card-deck"));
}

#[tokio::test]
async fn deck_import_anki_dry_run_sends_path_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/import/anki/dry-run"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"dryRun": true}
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "deck",
            "import",
            "anki",
            "/tmp/sample.apkg",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"dryRun\": true"));
}

#[tokio::test]
async fn deck_import_anki_confirm_sends_remote_media_choice_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/import/anki/confirm"))
        .and(body_json(serde_json::json!({
            "path": "/tmp/sample.apkg",
            "dryRunId": "dry-run-1",
            "skipCardsWithMissingMedia": true,
            "stripRemoteMedia": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"imported": true}
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "deck",
            "import",
            "anki",
            "/tmp/sample.apkg",
            "--confirm",
            "dry-run-1",
            "--skip-cards-with-missing-media",
            "--strip-remote-media",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"imported\": true"));
}

#[tokio::test]
async fn deck_export_anki_preflight_calls_preflight_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/deck-1/export/anki/preflight"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"preflight": "blocked"}
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "deck",
            "export",
            "anki",
            "deck-1",
            "--preflight",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"preflight\": \"blocked\""));
}

#[tokio::test]
async fn sends_bearer_token_and_maps_auth_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/health"))
        .and(header("authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "ok": false,
            "error": {"code": "token_required", "message": "Token required."}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "--token",
            "secret-token",
            "health",
        ])
        .assert()
        .code(4)
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["code"], "auth_error");
}

#[tokio::test]
async fn maps_missing_endpoint_to_api_gap() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/capabilities"))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "ok": false,
            "error": {"code": "not_found", "message": "Endpoint not found."}
        })))
        .expect(1)
        .mount(&server)
        .await;

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "api",
            "capabilities",
        ])
        .assert()
        .code(7)
        .stdout(predicate::str::contains("\"code\": \"api_gap\""));
}

#[test]
fn cli_known_paths_exist_in_openapi() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let openapi_path = manifest_dir.join("granoflow-local-http-api.openapi.yaml");
    let openapi = std::fs::read_to_string(openapi_path).unwrap();
    for path in CLI_KNOWN_PATHS {
        assert!(
            openapi.contains(path),
            "{path} missing from OpenAPI document"
        );
    }
}

#[test]
fn critical_openapi_paths_are_cli_covered_or_intentionally_unsupported() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let openapi_path = manifest_dir.join("granoflow-local-http-api.openapi.yaml");
    let openapi = std::fs::read_to_string(openapi_path).unwrap();
    for path in CRITICAL_OPENAPI_PATHS {
        assert!(
            openapi.contains(path),
            "{path} missing from OpenAPI document"
        );
        assert!(
            CLI_KNOWN_PATHS.contains(path)
                || INTENTIONALLY_UNSUPPORTED_OPENAPI_PATHS.contains(path),
            "{path} is critical but neither CLI-covered nor intentionally unsupported"
        );
    }
}
