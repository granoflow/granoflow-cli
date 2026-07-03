use assert_cmd::Command;
use granoflow::openapi_drift::{
    CLI_KNOWN_PATHS, CRITICAL_OPENAPI_PATHS, INTENTIONALLY_UNSUPPORTED_OPENAPI_PATHS,
};
use predicates::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use tempfile::{tempdir, NamedTempFile};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

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

#[tokio::test]
async fn review_month_show_uses_month_rest_route() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/reviews/months/2026-05"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "entity_type": "review.month",
            "entity": {"year": 2026, "month": 5, "content": "A useful month"}
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
            "review",
            "month",
            "show",
            "--month",
            "2026-05",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["data"]["entity"]["content"], "A useful month");
}

#[test]
fn review_month_update_dry_run_uses_month_patch_route() {
    let input = NamedTempFile::new().unwrap();
    std::fs::write(input.path(), r#"{"content":"A useful month"}"#).unwrap();
    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "review",
            "month",
            "update",
            "--month",
            "2026-05",
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
    assert_eq!(envelope["data"]["method"], "PATCH");
    assert_eq!(envelope["data"]["path"], "/v1/reviews/months/2026-05");
    assert_eq!(
        envelope["data"]["body"],
        serde_json::json!({"content": "A useful month"})
    );
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

#[test]
fn backup_encrypt_decrypt_is_offline_and_redacts_secret() {
    let temp = tempdir().unwrap();
    let plaintext = temp.path().join("plain.flow.grano");
    let encrypted = temp.path().join("encrypted.flow.grano");
    let decrypted = temp.path().join("decrypted.flow.grano");
    let secret_file = temp.path().join("secret.txt");
    let secret = "restore-key-should-not-appear";
    std::fs::write(&secret_file, secret).unwrap();
    write_plaintext_backup(&plaintext, "Original").unwrap();

    let encrypt_output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "backup",
            "encrypt",
            "--input",
            plaintext.to_str().unwrap(),
            "--output",
            encrypted.to_str().unwrap(),
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let encrypt_envelope: Value = serde_json::from_slice(&encrypt_output).unwrap();
    assert_eq!(encrypt_envelope["data"]["packageKind"], "encrypted");
    assert!(!String::from_utf8_lossy(&encrypt_output).contains(secret));

    let decrypt_output = Command::cargo_bin("granoflow")
        .unwrap()
        .env("GRANOFLOW_TEST_BACKUP_SECRET", secret)
        .args([
            "--json",
            "--config",
            "/path/that/must/not/be/read.toml",
            "backup",
            "decrypt",
            "--input",
            encrypted.to_str().unwrap(),
            "--output",
            decrypted.to_str().unwrap(),
            "--secret-env",
            "GRANOFLOW_TEST_BACKUP_SECRET",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let decrypt_envelope: Value = serde_json::from_slice(&decrypt_output).unwrap();
    assert_eq!(decrypt_envelope["data"]["packageKind"], "plaintext");
    assert_eq!(decrypt_envelope["data"]["secretSource"], "env");
    assert!(!String::from_utf8_lossy(&decrypt_output).contains(secret));

    let manifest = read_zip_json(&decrypted, "manifest.json").unwrap();
    assert_eq!(manifest["format_version"], 3);
    assert_eq!(manifest["package_kind"], "plaintext");
    assert!(manifest.get("keyring").is_none());
    assert!(manifest["privacy_warning"]
        .as_str()
        .unwrap()
        .contains("expose private records"));
    let records = read_zip_string(&decrypted, "records/tasks.ndjson").unwrap();
    assert!(records.contains("Original"));
    assert!(!records.contains("\"payload\""));
}

#[test]
fn backup_decrypt_rejects_old_backup_versions() {
    let temp = tempdir().unwrap();
    let old_package = temp.path().join("old.flow.grano");
    let output = temp.path().join("out.flow.grano");
    let secret_file = temp.path().join("secret.txt");
    std::fs::write(&secret_file, "restore-key").unwrap();
    write_plaintext_backup_with_version(&old_package, "Old", 2).unwrap();

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "backup",
            "decrypt",
            "--input",
            old_package.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--secret-file",
            secret_file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "unsupported backup format_version 2",
        ));
}

#[test]
fn deck_anki_preview_is_offline_and_ignores_broken_config() {
    let temp = tempdir().unwrap();
    let apkg = temp.path().join("sample.apkg");
    let bad_config = temp.path().join("bad-config.toml");
    std::fs::write(&bad_config, "not valid toml =").unwrap();
    write_simple_apkg(&apkg).unwrap();

    let output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--config",
            bad_config.to_str().unwrap(),
            "deck",
            "anki",
            "preview",
            "--input",
            apkg.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["command"], "deck anki preview");
    assert_eq!(envelope["data"]["decision"], "can_convert");
    assert_eq!(envelope["data"]["deckCount"], 1);
    assert_eq!(envelope["data"]["noteCount"], 1);
    assert_eq!(envelope["data"]["cardCount"], 1);
    assert_eq!(envelope["data"]["convertibleCardCount"], 1);
    assert_eq!(envelope["data"]["mediaCount"], 1);
}

#[test]
fn deck_anki_preview_and_convert_explain_rejected_deck() {
    let temp = tempdir().unwrap();
    let apkg = temp.path().join("unsupported.apkg");
    let output = temp.path().join("converted.deck.grano");
    write_unconvertible_apkg(&apkg).unwrap();

    let preview_output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "deck",
            "anki",
            "preview",
            "--input",
            apkg.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&preview_output).unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["decision"], "rejected");
    assert_eq!(envelope["data"]["reason"], "no_convertible_cards");
    assert_eq!(envelope["data"]["convertibleCardCount"], 0);
    assert_eq!(envelope["data"]["skippedCardCount"], 1);
    assert!(envelope["data"]["message"]
        .as_str()
        .unwrap()
        .contains("cannot be converted yet"));
    assert!(envelope["data"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .any(|detail| detail.as_str().unwrap().contains("Found 1 Anki card(s)")));
    assert!(envelope["data"]["nextAction"]
        .as_str()
        .unwrap()
        .contains("Front/Back"));

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "deck",
            "anki",
            "convert",
            "--input",
            apkg.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("cannot be converted yet"))
        .stdout(predicate::str::contains("no_convertible_cards"))
        .stdout(predicate::str::contains("Front/Back"));
}

#[test]
fn deck_anki_convert_writes_app_readable_deck_package_and_rejects_overwrite() {
    let temp = tempdir().unwrap();
    let apkg = temp.path().join("sample.apkg");
    let output = temp.path().join("converted.deck.grano");
    write_simple_apkg(&apkg).unwrap();

    let convert_output = Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "deck",
            "anki",
            "convert",
            "--input",
            apkg.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let envelope: Value = serde_json::from_slice(&convert_output).unwrap();
    assert_eq!(envelope["data"]["command"], "deck anki convert");
    assert_eq!(envelope["data"]["cardCount"], 1);
    assert_eq!(envelope["data"]["mediaCount"], 1);

    let manifest = read_zip_json(&output, "manifest.json").unwrap();
    assert_eq!(manifest["artifact_type"], "granoflow.review_card_deck");
    assert_eq!(manifest["schema_version"], 6);
    assert_eq!(manifest["card_count"], 1);
    assert_eq!(manifest["media_count"], 1);
    assert!(!manifest["package_sha"].as_str().unwrap().is_empty());
    assert_eq!(manifest["top_deck"]["name"], "Imported");

    let content = read_zip_json(&output, "content.json").unwrap();
    assert_eq!(content["cards"][0]["front"], "What is Granoflow?");
    assert_eq!(content["cards"][0]["back"], "A local-first planning app");
    assert_eq!(content["cards"][0]["source_kind"], "deck_apkg_import");
    assert_eq!(content["decks"][0]["display_name"], "Imported");
    assert!(content["review_note_types"].as_array().unwrap().is_empty());

    let media_manifest = read_zip_json(&output, "media_manifest.json").unwrap();
    assert_eq!(media_manifest["items"][0]["original_filename"], "image.png");
    assert_eq!(media_manifest["items"][0]["mime_type"], "image/png");

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "deck",
            "anki",
            "convert",
            "--input",
            apkg.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("output package already exists"));
}

fn write_plaintext_backup(path: &Path, title: &str) -> std::io::Result<()> {
    write_plaintext_backup_with_version(path, title, 3)
}

fn write_plaintext_backup_with_version(
    path: &Path,
    title: &str,
    format_version: i64,
) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    let manifest = serde_json::json!({
        "format_version": format_version,
        "package_kind": "plaintext",
        "created_at_ms": 1,
        "app_version": "test",
        "library_id": "test-library",
        "tables": [
            {"name": "tasks", "path": "records/tasks.ndjson", "count": 1}
        ],
        "attachments": {
            "images_root": "attachments/images",
            "pdfs_root": "attachments/pdfs",
            "files_root": "attachments/files"
        }
    });
    zip.start_file("manifest.json", options)?;
    zip.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())?;
    zip.start_file("records/tasks.ndjson", options)?;
    writeln!(
        zip,
        "{}",
        serde_json::json!({"id": "task-1", "title": title})
    )?;
    zip.finish()?;
    Ok(())
}

fn read_zip_json(path: &Path, entry_name: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let raw = read_zip_string(path, entry_name)?;
    Ok(serde_json::from_str(&raw)?)
}

fn read_zip_string(path: &Path, entry_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entry = archive.by_name(entry_name)?;
    let mut raw = String::new();
    entry.read_to_string(&mut raw)?;
    Ok(raw)
}

fn write_simple_apkg(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let collection_path = temp.path().join("collection.anki2");
    let db = Connection::open(&collection_path)?;
    db.execute_batch(
        r#"
        CREATE TABLE col (decks TEXT NOT NULL, models TEXT NOT NULL);
        CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
        CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL);
        "#,
    )?;
    let decks = serde_json::json!({
        "1": {"name": "Default"},
        "2": {"name": "Focus::Imported"}
    });
    let models = serde_json::json!({
        "1": {
            "name": "Basic",
            "flds": [{"name": "Front"}, {"name": "Back"}],
            "tmpls": [{"name": "Card 1"}]
        }
    });
    db.execute(
        "INSERT INTO col (decks, models) VALUES (?1, ?2)",
        [&decks.to_string(), &models.to_string()],
    )?;
    db.execute(
        "INSERT INTO notes (id, mid, tags, flds) VALUES (10, 1, '', ?1)",
        ["What is Granoflow?\u{001f}A local-first planning app"],
    )?;
    db.execute(
        "INSERT INTO cards (id, nid, did, ord) VALUES (20, 10, 2, 0)",
        [],
    )?;
    drop(db);

    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("collection.anki2", options)?;
    zip.write_all(&std::fs::read(collection_path)?)?;
    zip.start_file("media", options)?;
    zip.write_all(br#"{"0":"image.png"}"#)?;
    zip.start_file("0", options)?;
    zip.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])?;
    zip.finish()?;
    Ok(())
}

fn write_unconvertible_apkg(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempdir()?;
    let collection_path = temp.path().join("collection.anki2");
    let db = Connection::open(&collection_path)?;
    db.execute_batch(
        r#"
        CREATE TABLE col (decks TEXT NOT NULL, models TEXT NOT NULL);
        CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, tags TEXT NOT NULL, flds TEXT NOT NULL);
        CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, did INTEGER NOT NULL, ord INTEGER NOT NULL);
        "#,
    )?;
    let decks = serde_json::json!({
        "1": {"name": "Default"}
    });
    let models = serde_json::json!({
        "1": {
            "name": "One-sided",
            "flds": [{"name": "Text"}],
            "tmpls": [{"name": "Card 1"}]
        }
    });
    db.execute(
        "INSERT INTO col (decks, models) VALUES (?1, ?2)",
        [&decks.to_string(), &models.to_string()],
    )?;
    db.execute(
        "INSERT INTO notes (id, mid, tags, flds) VALUES (10, 1, '', ?1)",
        ["Only one visible side"],
    )?;
    db.execute(
        "INSERT INTO cards (id, nid, did, ord) VALUES (20, 10, 1, 0)",
        [],
    )?;
    drop(db);

    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();
    zip.start_file("collection.anki2", options)?;
    zip.write_all(&std::fs::read(collection_path)?)?;
    zip.start_file("media", options)?;
    zip.write_all(br#"{}"#)?;
    zip.finish()?;
    Ok(())
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
async fn api_sync_commands_call_local_http_api_sync_endpoints() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/sync/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"isAvailable": true}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/sync/push"))
        .and(body_json(serde_json::json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"success": true}
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
            "sync",
            "status",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"isAvailable\": true"));

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "api",
            "sync",
            "push",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"success\": true"));
}

#[tokio::test]
async fn api_backup_commands_call_app_backup_endpoints() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/backup/exports"))
        .and(body_json(serde_json::json!({
            "outputPath": "/tmp/out.flow.grano"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"outputPath": "/tmp/out.flow.grano"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/backup/imports"))
        .and(body_json(serde_json::json!({
            "inputPath": "/tmp/out.flow.grano",
            "secretFile": "/tmp/secret.txt",
            "allowMissingAttachments": false,
            "allowLargeAttachmentConversion": false,
            "confirm": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"inputPath": "/tmp/out.flow.grano"}
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
            "backup",
            "export",
            "--output",
            "/tmp/out.flow.grano",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("/tmp/out.flow.grano"));

    Command::cargo_bin("granoflow")
        .unwrap()
        .args([
            "--json",
            "--api-base-url",
            &server.uri(),
            "api",
            "backup",
            "restore",
            "--input",
            "/tmp/out.flow.grano",
            "--secret-file",
            "/tmp/secret.txt",
            "--confirm",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("/tmp/out.flow.grano"));
}

#[tokio::test]
async fn api_test_seed_command_uses_command_envelope_not_sqlite() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/commands"))
        .and(body_json(serde_json::json!({
            "command": "test-seed-sync-backup-coverage",
            "arguments": {"run_id": "run-1"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"run_id": "run-1"}
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
            "test",
            "seed-sync-backup-coverage",
            "--run-id",
            "run-1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"run_id\": \"run-1\""));
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
async fn deck_package_preview_sends_path_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/import/package/preview"))
        .and(body_json(serde_json::json!({
            "path": "/tmp/sample.deck.grano"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"entity_type": "review-card-deck-package-import-preview"}
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
            "package",
            "preview",
            "/tmp/sample.deck.grano",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "review-card-deck-package-import-preview",
        ));
}

#[tokio::test]
async fn deck_package_import_sends_study_history_choice_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/import/package/confirm"))
        .and(body_json(serde_json::json!({
            "path": "/tmp/sample.deck.grano",
            "importStudyHistory": true
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
            "package",
            "import",
            "/tmp/sample.deck.grano",
            "--import-study-history",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"imported\": true"));
}

#[tokio::test]
async fn deck_package_export_sends_output_config_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-card-decks/export/package"))
        .and(body_json(serde_json::json!({
            "deckId": "deck-1",
            "outPath": "/tmp/out.deck.grano",
            "author": "Will",
            "contact": "will@example.com",
            "version": "1.0",
            "includeStudyHistory": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"exported": true}
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
            "package",
            "export",
            "--deck-id",
            "deck-1",
            "--out-path",
            "/tmp/out.deck.grano",
            "--author",
            "Will",
            "--contact",
            "will@example.com",
            "--version",
            "1.0",
            "--include-study-history",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"exported\": true"));
}

#[tokio::test]
async fn card_action_commands_call_review_card_endpoints() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/review-cards/card-1/archive"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"changed": true}
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
            "card",
            "archive",
            "card-1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"changed\": true"));
}

#[tokio::test]
async fn card_unlink_command_calls_task_scoped_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/tasks/task-1/review-cards/card-1/unlink"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ok": true,
            "data": {"changed": true}
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
            "card",
            "unlink",
            "--task-id",
            "task-1",
            "--card-id",
            "card-1",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"changed\": true"));
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
