use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::errors::{CliError, CliResult};

const ARTIFACT_TYPE: &str = "granoflow.review_card_deck";
const SCHEMA_VERSION: i64 = 6;

#[derive(Debug)]
pub struct DeckPackageDraft {
    pub top_deck_name: String,
    pub decks: Vec<Value>,
    pub cards: Vec<Value>,
    pub media: Vec<DeckPackageMedia>,
}

#[derive(Debug)]
pub struct DeckPackageMedia {
    pub media_asset_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct DeckPackageWriteResult {
    pub package_id: String,
    pub package_sha: String,
    pub deck_count: usize,
    pub card_count: usize,
    pub media_count: usize,
}

pub fn write_deck_package(
    draft: &DeckPackageDraft,
    output: &str,
) -> CliResult<DeckPackageWriteResult> {
    require_output_path(output)?;
    if Path::new(output).exists() {
        return Err(CliError::Usage(format!(
            "output package already exists: {output}"
        )));
    }

    let package_id = Uuid::new_v4().to_string();
    let now = now_ms()?;
    let media_manifest = media_manifest(&draft.media);
    let content = package_content(draft, now);
    let mut manifest = package_manifest(
        &package_id,
        now,
        &draft.top_deck_name,
        draft.decks.len(),
        draft.cards.len(),
        draft.media.len(),
    );
    let package_sha = package_sha_for(&manifest, &content, &media_manifest)?;
    manifest["package_sha"] = json!(package_sha);

    let file = fs::File::create(output)
        .map_err(|error| CliError::Usage(format!("failed to create output package: {error}")))?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    write_json_entry(&mut writer, "manifest.json", &manifest, options)?;
    write_json_entry(&mut writer, "content.json", &content, options)?;
    write_json_entry(&mut writer, "media_manifest.json", &media_manifest, options)?;
    for media in &draft.media {
        let path = media_path(&media.media_asset_id);
        writer.start_file(path, options).map_err(zip_error)?;
        writer.write_all(&media.bytes).map_err(io_error)?;
    }
    writer.finish().map_err(zip_error)?;

    Ok(DeckPackageWriteResult {
        package_id,
        package_sha,
        deck_count: draft.decks.len(),
        card_count: draft.cards.len(),
        media_count: draft.media.len(),
    })
}

pub fn require_output_path(output: &str) -> CliResult<()> {
    let path = PathBuf::from(output);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !file_name.to_lowercase().ends_with(".deck.grano") {
        return Err(CliError::Usage(
            "output package must use the .deck.grano extension".to_string(),
        ));
    }
    Ok(())
}

fn package_content(draft: &DeckPackageDraft, now: i64) -> Value {
    json!({
        "exported_at": now,
        "source_summary": draft.top_deck_name,
        "decks": draft.decks,
        "cards": draft.cards,
        "review_note_types": [],
        "review_notes": [],
        "review_note_field_values": [],
        "review_note_fields": [],
        "review_card_templates": [],
        "review_field_media_refs": [],
        "review_media_assets": [],
    })
}

fn package_manifest(
    package_id: &str,
    now: i64,
    deck_name: &str,
    deck_count: usize,
    card_count: usize,
    media_count: usize,
) -> Value {
    json!({
        "artifact_type": ARTIFACT_TYPE,
        "schema_version": SCHEMA_VERSION,
        "package_id": package_id,
        "package_sha": "",
        "exported_at": now,
        "app_version": null,
        "top_deck": {
            "id": "deck-apkg-root",
            "name": deck_name,
            "author": "",
            "contact": "",
            "version": "1.0",
        },
        "deck_count": deck_count,
        "card_count": card_count,
        "media_count": media_count,
        "includes_study_history": false,
    })
}

fn media_manifest(media: &[DeckPackageMedia]) -> Value {
    let items = media
        .iter()
        .map(|item| {
            json!({
                "media_asset_id": item.media_asset_id,
                "path": media_path(&item.media_asset_id),
                "sha256": sha256_hex(&item.bytes),
                "byte_size": item.bytes.len(),
                "mime_type": item.mime_type,
                "original_filename": item.original_filename,
            })
        })
        .collect::<Vec<_>>();
    json!({"version": 1, "items": items})
}

fn media_path(media_asset_id: &str) -> String {
    format!("media/{media_asset_id}.bin")
}

fn package_sha_for(manifest: &Value, content: &Value, media_manifest: &Value) -> CliResult<String> {
    let mut manifest_for_hash = manifest.clone();
    manifest_for_hash["package_sha"] = json!("");
    let raw = format!(
        "{{\"manifest\":{},\"content\":{},\"media_manifest\":{}}}",
        json_string(&sort_json(manifest_for_hash))?,
        json_string(&sort_json(content.clone()))?,
        json_string(&sort_json(media_manifest.clone()))?,
    );
    Ok(sha256_hex(raw.as_bytes()))
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json).collect()),
        Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            json!(sorted)
        }
        other => other,
    }
}

fn write_json_entry(
    writer: &mut ZipWriter<fs::File>,
    name: &str,
    value: &Value,
    options: SimpleFileOptions,
) -> CliResult<()> {
    writer.start_file(name, options).map_err(zip_error)?;
    writer
        .write_all(&serde_json::to_vec(value).map_err(internal_json_error)?)
        .map_err(io_error)?;
    Ok(())
}

fn json_string(value: &Value) -> CliResult<String> {
    serde_json::to_string(value).map_err(internal_json_error)
}

fn now_ms() -> CliResult<i64> {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| CliError::Internal(format!("system clock before epoch: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| CliError::Internal("system time does not fit i64 milliseconds".to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn zip_error(error: zip::result::ZipError) -> CliError {
    CliError::Usage(format!("failed to write deck package zip: {error}"))
}

fn io_error(error: std::io::Error) -> CliError {
    CliError::Usage(format!("failed to write deck package: {error}"))
}

fn internal_json_error(error: serde_json::Error) -> CliError {
    CliError::Internal(format!("failed to serialize deck package JSON: {error}"))
}
