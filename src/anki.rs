use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use rusqlite::Connection;
use serde_json::{json, Value};
use uuid::Uuid;
use zip::ZipArchive;

use crate::{
    cli::{DeckAnkiCommand, DeckAnkiSubcommand},
    deck_package::{require_output_path, write_deck_package, DeckPackageDraft, DeckPackageMedia},
    errors::{CliError, CliResult},
};

const FIELD_SEPARATOR: char = '\u{001f}';
const MAX_ARCHIVE_FILES: usize = 10_000;
const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;

pub fn run_deck_anki(command: &DeckAnkiCommand) -> CliResult<Value> {
    match &command.command {
        DeckAnkiSubcommand::Preview(args) => {
            let package = AnkiPackage::read(&args.input)?;
            let conversion = package.convert(false)?;
            Ok(conversion.preview_json(&args.input))
        }
        DeckAnkiSubcommand::Convert(args) => {
            require_output_path(&args.output)?;
            let package = AnkiPackage::read(&args.input)?;
            let conversion = package.convert(true)?;
            if conversion.cards.is_empty() {
                return Err(CliError::UnsupportedFeature(
                    conversion.no_convertible_error_message(),
                ));
            }
            let result = write_deck_package(&conversion.draft, &args.output)?;
            Ok(json!({
                "command": "deck anki convert",
                "input": args.input,
                "output": args.output,
                "packageId": result.package_id,
                "packageSha": result.package_sha,
                "deckCount": result.deck_count,
                "noteCount": conversion.note_count,
                "cardCount": result.card_count,
                "convertibleCardCount": conversion.cards.len(),
                "skippedCardCount": conversion.skipped_card_count(),
                "mediaCount": result.media_count,
                "policyBlocks": conversion.policy_blocks,
            }))
        }
    }
}

struct AnkiPackage {
    collection_bytes: Vec<u8>,
    media_manifest: BTreeMap<String, String>,
    media_entries: HashMap<String, Vec<u8>>,
}

impl AnkiPackage {
    fn read(input: &str) -> CliResult<Self> {
        let file = fs::File::open(input)
            .map_err(|error| CliError::Usage(format!("failed to open apkg: {error}")))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| CliError::Usage(format!("invalid apkg zip: {error}")))?;
        validate_archive(&mut archive)?;
        let collection_bytes = read_collection_bytes(&mut archive)?;
        let media_manifest = read_media_manifest(&mut archive)?;
        let media_entries = read_media_entries(&mut archive, &media_manifest)?;
        Ok(Self {
            collection_bytes,
            media_manifest,
            media_entries,
        })
    }

    fn convert(&self, include_media_bytes: bool) -> CliResult<AnkiConversion> {
        let temp_path = write_temp_collection(&self.collection_bytes)?;
        let result = self.convert_from_temp_db(&temp_path, include_media_bytes);
        let _ = fs::remove_file(&temp_path);
        result
    }

    fn convert_from_temp_db(
        &self,
        temp_path: &Path,
        include_media_bytes: bool,
    ) -> CliResult<AnkiConversion> {
        let db = Connection::open(temp_path)
            .map_err(|error| CliError::Usage(format!("invalid anki collection sqlite: {error}")))?;
        let collection = read_collection(&db)?;
        let draft = build_deck_package_draft(&collection, self, include_media_bytes);
        let policy_blocks = media_policy_blocks(&self.media_manifest, &self.media_entries);
        Ok(AnkiConversion {
            deck_count: collection.deck_count,
            note_count: collection.notes.len(),
            card_count: collection.cards.len(),
            cards: draft.cards.clone(),
            draft,
            policy_blocks,
        })
    }
}

struct AnkiConversion {
    deck_count: usize,
    note_count: usize,
    card_count: usize,
    cards: Vec<Value>,
    draft: DeckPackageDraft,
    policy_blocks: Vec<Value>,
}

struct AnkiRejectionReport {
    reason: &'static str,
    message: String,
    details: Vec<String>,
    next_action: &'static str,
}

impl AnkiRejectionReport {
    fn to_json(&self) -> Value {
        json!({
            "reason": self.reason,
            "message": self.message,
            "details": self.details,
            "nextAction": self.next_action,
        })
    }
}

impl AnkiConversion {
    fn skipped_card_count(&self) -> usize {
        self.card_count.saturating_sub(self.cards.len())
    }

    fn preview_json(&self, input: &str) -> Value {
        let mut preview = json!({
            "command": "deck anki preview",
            "input": input,
            "decision": if self.cards.is_empty() { "rejected" } else { "can_convert" },
            "deckCount": self.deck_count,
            "noteCount": self.note_count,
            "cardCount": self.card_count,
            "convertibleCardCount": self.cards.len(),
            "skippedCardCount": self.skipped_card_count(),
            "mediaCount": self.draft.media.len(),
            "policyBlocks": self.policy_blocks,
        });
        if let Some(report) = self.rejection_report() {
            if let Some(object) = preview.as_object_mut() {
                let report = report.to_json();
                object.insert("reason".to_string(), report["reason"].clone());
                object.insert("message".to_string(), report["message"].clone());
                object.insert("details".to_string(), report["details"].clone());
                object.insert("nextAction".to_string(), report["nextAction"].clone());
            }
        }
        preview
    }

    fn no_convertible_error_message(&self) -> String {
        if let Some(report) = self.rejection_report() {
            format!(
                "{} Reason: {}. Details: {} Next step: {}",
                report.message,
                report.reason,
                report.details.join(" "),
                report.next_action,
            )
        } else {
            "This Anki deck cannot be converted yet.".to_string()
        }
    }

    fn rejection_report(&self) -> Option<AnkiRejectionReport> {
        if !self.cards.is_empty() {
            return None;
        }
        let mut details = vec![format!(
            "Found {} Anki card(s), but 0 match the currently supported question-and-answer card format.",
            self.card_count
        )];
        if self.note_count > 0 {
            details.push(format!("Found {} Anki note(s).", self.note_count));
        }
        let unsupported_media = policy_block_count(&self.policy_blocks, "unsupported_media");
        if unsupported_media > 0 {
            details.push(format!(
                "Found {unsupported_media} unsupported media reference(s); image files are supported, but this converter cannot use every Anki media format yet."
            ));
        }
        Some(AnkiRejectionReport {
            reason: "no_convertible_cards",
            message: "This Anki deck cannot be converted yet because none of its cards look like a supported question-and-answer card.".to_string(),
            details,
            next_action: "Try a basic Front/Back Anki deck, or update this deck so each card has clear Front and Back/Answer fields before converting again.",
        })
    }
}

fn policy_block_count(blocks: &[Value], code: &str) -> i64 {
    blocks
        .iter()
        .find(|block| block["code"] == code)
        .and_then(|block| block["count"].as_i64())
        .unwrap_or_default()
}

#[derive(Clone)]
struct CollectionData {
    deck_count: usize,
    decks: BTreeMap<i64, DeckRow>,
    models: BTreeMap<i64, ModelRow>,
    notes: BTreeMap<i64, NoteRow>,
    cards: Vec<CardRow>,
}

#[derive(Clone)]
struct DeckRow {
    id: i64,
    name: String,
}

#[derive(Clone)]
struct ModelRow {
    fields: Vec<String>,
    templates: Vec<String>,
}

#[derive(Clone)]
struct NoteRow {
    id: i64,
    model_id: i64,
    fields: Vec<String>,
}

#[derive(Clone)]
struct CardRow {
    id: i64,
    note_id: i64,
    deck_id: i64,
    ordinal: i64,
}

fn validate_archive(archive: &mut ZipArchive<fs::File>) -> CliResult<()> {
    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(CliError::Usage("apkg contains too many files".to_string()));
    }
    let mut seen = BTreeSet::new();
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| CliError::Usage(format!("invalid apkg entry: {error}")))?;
        let name = normalize_zip_name(entry.name())?;
        if !seen.insert(name) {
            return Err(CliError::Usage(
                "apkg contains duplicate file paths".to_string(),
            ));
        }
        total_size = total_size.saturating_add(entry.size());
        if total_size > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
            return Err(CliError::Usage(
                "apkg uncompressed size is too large".to_string(),
            ));
        }
    }
    Ok(())
}

fn read_collection_bytes(archive: &mut ZipArchive<fs::File>) -> CliResult<Vec<u8>> {
    for candidate in ["collection.anki21", "collection.anki2"] {
        if let Ok(mut entry) = archive.by_name(candidate) {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| CliError::Usage(format!("failed to read {candidate}: {error}")))?;
            return Ok(bytes);
        }
    }
    Err(CliError::Usage(
        "apkg is missing collection.anki21 or collection.anki2".to_string(),
    ))
}

fn read_media_manifest(archive: &mut ZipArchive<fs::File>) -> CliResult<BTreeMap<String, String>> {
    let Ok(mut entry) = archive.by_name("media") else {
        return Ok(BTreeMap::new());
    };
    let mut raw = String::new();
    entry
        .read_to_string(&mut raw)
        .map_err(|error| CliError::Usage(format!("failed to read apkg media manifest: {error}")))?;
    let decoded: Value = serde_json::from_str(&raw)
        .map_err(|error| CliError::Usage(format!("apkg media manifest must be JSON: {error}")))?;
    let mut result = BTreeMap::new();
    if let Value::Object(map) = decoded {
        for (key, value) in map {
            if let Some(name) = value.as_str() {
                result.insert(key, name.to_string());
            }
        }
    }
    Ok(result)
}

fn read_media_entries(
    archive: &mut ZipArchive<fs::File>,
    manifest: &BTreeMap<String, String>,
) -> CliResult<HashMap<String, Vec<u8>>> {
    let mut entries = HashMap::new();
    for key in manifest.keys() {
        if let Ok(mut entry) = archive.by_name(key) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|error| {
                CliError::Usage(format!("failed to read apkg media entry {key}: {error}"))
            })?;
            entries.insert(key.clone(), bytes);
        }
    }
    Ok(entries)
}

fn read_collection(db: &Connection) -> CliResult<CollectionData> {
    let (raw_decks, raw_models) = db
        .query_row("SELECT decks, models FROM col LIMIT 1", [], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| {
            CliError::Usage(format!("failed to read anki collection metadata: {error}"))
        })?;
    let notes = read_notes(db)?;
    let cards = read_cards(db)?;
    let referenced_deck_ids = cards
        .iter()
        .filter_map(|card| (card.deck_id != 0).then_some(card.deck_id))
        .collect::<BTreeSet<_>>();
    let raw_decks = parse_decks(&raw_decks)?;
    let decks = if referenced_deck_ids.is_empty() {
        raw_decks
    } else {
        raw_decks
            .into_iter()
            .filter(|(id, _)| referenced_deck_ids.contains(id))
            .collect()
    };
    let deck_count = if referenced_deck_ids.is_empty() {
        decks.len()
    } else {
        referenced_deck_ids.len()
    };
    Ok(CollectionData {
        deck_count,
        decks,
        models: parse_models(&raw_models)?,
        notes,
        cards,
    })
}

fn read_notes(db: &Connection) -> CliResult<BTreeMap<i64, NoteRow>> {
    let mut statement = db
        .prepare("SELECT id, mid, flds FROM notes ORDER BY id ASC")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            let raw_fields = row.get::<_, String>(2)?;
            Ok(NoteRow {
                id: row.get(0)?,
                model_id: row.get(1)?,
                fields: raw_fields
                    .split(FIELD_SEPARATOR)
                    .map(str::to_string)
                    .collect(),
            })
        })
        .map_err(sql_error)?;
    let mut notes = BTreeMap::new();
    for row in rows {
        let note = row.map_err(sql_error)?;
        notes.insert(note.id, note);
    }
    Ok(notes)
}

fn read_cards(db: &Connection) -> CliResult<Vec<CardRow>> {
    let mut statement = db
        .prepare("SELECT id, nid, did, ord FROM cards ORDER BY id ASC")
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok(CardRow {
                id: row.get(0)?,
                note_id: row.get(1)?,
                deck_id: row.get(2)?,
                ordinal: row.get(3)?,
            })
        })
        .map_err(sql_error)?;
    let mut cards = Vec::new();
    for row in rows {
        cards.push(row.map_err(sql_error)?);
    }
    Ok(cards)
}

fn parse_decks(raw: &str) -> CliResult<BTreeMap<i64, DeckRow>> {
    let decoded: Value = serde_json::from_str(raw)
        .map_err(|error| CliError::Usage(format!("invalid decks JSON: {error}")))?;
    let mut decks = BTreeMap::new();
    if let Value::Object(map) = decoded {
        for (id, value) in map {
            let parsed_id = id.parse::<i64>().unwrap_or_default();
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .trim()
                .to_string();
            decks.insert(
                parsed_id,
                DeckRow {
                    id: parsed_id,
                    name,
                },
            );
        }
    }
    Ok(decks)
}

fn parse_models(raw: &str) -> CliResult<BTreeMap<i64, ModelRow>> {
    let decoded: Value = serde_json::from_str(raw)
        .map_err(|error| CliError::Usage(format!("invalid models JSON: {error}")))?;
    let mut models = BTreeMap::new();
    if let Value::Object(map) = decoded {
        for (id, value) in map {
            let parsed_id = id.parse::<i64>().unwrap_or_default();
            let fields = value
                .get("flds")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, field)| {
                            field
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or(if index == 0 { "Front" } else { "Back" })
                                .to_string()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let templates = value
                .get("tmpls")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(|template| {
                            template
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("Card")
                                .to_string()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            models.insert(parsed_id, ModelRow { fields, templates });
        }
    }
    Ok(models)
}

fn build_deck_package_draft(
    collection: &CollectionData,
    package: &AnkiPackage,
    include_media_bytes: bool,
) -> DeckPackageDraft {
    let now = current_ms_for_rows();
    let top_deck_name = top_deck_name(collection);
    let decks = collection
        .decks
        .values()
        .map(|deck| deck_json(deck, now))
        .collect::<Vec<_>>();
    let cards = collection
        .cards
        .iter()
        .filter_map(|card| card_json(card, collection, now))
        .collect::<Vec<_>>();
    let media = package_media(package, include_media_bytes);
    DeckPackageDraft {
        top_deck_name,
        decks: if decks.is_empty() {
            vec![deck_json(
                &DeckRow {
                    id: 1,
                    name: "Anki Cards".to_string(),
                },
                now,
            )]
        } else {
            decks
        },
        cards,
        media,
    }
}

fn deck_json(deck: &DeckRow, now: i64) -> Value {
    let id = format!("deck-apkg-{}", deck.id);
    json!({
        "id": id,
        "parent_deck_id": null,
        "display_name": deck.name.split("::").last().unwrap_or(&deck.name),
        "source_kind": "deck_apkg_import",
        "source_id": format!("deck_apkg_import:{}", deck.id),
        "deck_slug": format!("anki/{}", deck.id),
        "is_system_managed": 0,
        "sort_index": 0.0,
        "created_at": now,
        "updated_at": now,
    })
}

fn card_json(card: &CardRow, collection: &CollectionData, now: i64) -> Option<Value> {
    let note = collection.notes.get(&card.note_id)?;
    let model = collection.models.get(&note.model_id);
    let front = field_value(
        &note.fields,
        model,
        &["front", "prompt", "question", "term", "text"],
        false,
    );
    let back = field_value(
        &note.fields,
        model,
        &["back", "answer", "definition", "extra"],
        true,
    );
    let front = sanitize_text(&front);
    let back = sanitize_text(&back);
    if front.is_empty() || back.is_empty() || front == back {
        return None;
    }
    Some(json!({
        "id": format!("anki-card-{}", card.id),
        "deck_id": format!("deck-apkg-{}", card.deck_id),
        "type": "basic_qa",
        "front": front,
        "back": back,
        "front_translation": null,
        "back_translation": null,
        "cloze_text": null,
        "cloze_answer": null,
        "cloze_text_translation": null,
        "cloze_answer_translation": null,
        "content_language": null,
        "translation_locale": null,
        "translation_source": null,
        "source_kind": "deck_apkg_import",
        "source_id": format!("deck_apkg_import:card:{}", card.id),
        "source_summary": format!("Anki card {}", card.id),
        "ai_state": null,
        "created_at": now,
        "updated_at": now,
        "archived_at": null,
        "note_id": format!("anki-note-{}", note.id),
        "template_id": template_id(model, card.ordinal),
        "front_field_value_id": null,
        "back_field_value_id": null,
        "content_schema_version": 1,
        "layout_blocks_json": null,
        "front_layout_json": null,
        "back_layout_json": null,
    }))
}

fn template_id(model: Option<&ModelRow>, ordinal: i64) -> Value {
    let name = model
        .and_then(|model| {
            model
                .templates
                .get(usize::try_from(ordinal).unwrap_or_default())
        })
        .cloned()
        .unwrap_or_else(|| "Card".to_string());
    json!(format!("anki-template-{}", normalize_slug(&name)))
}

fn field_value(
    fields: &[String],
    model: Option<&ModelRow>,
    preferred: &[&str],
    from_end: bool,
) -> String {
    if let Some(model) = model {
        for preferred_name in preferred {
            for (index, name) in model.fields.iter().enumerate() {
                if name.to_lowercase().contains(preferred_name) {
                    if let Some(value) = fields.get(index) {
                        if !value.trim().is_empty() {
                            return value.clone();
                        }
                    }
                }
            }
        }
    }
    let mut values: Box<dyn Iterator<Item = &String>> = if from_end {
        Box::new(fields.iter().rev())
    } else {
        Box::new(fields.iter())
    };
    values
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn sanitize_text(value: &str) -> String {
    let without_media = strip_media_tokens(value);
    let without_tags = strip_html_tags(&without_media);
    decode_basic_entities(&without_tags).trim().to_string()
}

fn strip_media_tokens(value: &str) -> String {
    let mut result = String::new();
    let mut remaining = value;
    while let Some(start) = remaining.find("[sound:") {
        result.push_str(&remaining[..start]);
        let after_start = &remaining[start + "[sound:".len()..];
        if let Some(end) = after_start.find(']') {
            remaining = &after_start[end + 1..];
        } else {
            remaining = "";
        }
    }
    result.push_str(remaining);
    result
}

fn strip_html_tags(value: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                result.push(' ');
            }
            _ if !in_tag => result.push(character),
            _ => {}
        }
    }
    result
}

fn decode_basic_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn package_media(package: &AnkiPackage, include_media_bytes: bool) -> Vec<DeckPackageMedia> {
    package
        .media_manifest
        .iter()
        .filter_map(|(key, filename)| {
            if !is_image(filename) {
                return None;
            }
            let bytes = if include_media_bytes {
                package.media_entries.get(key).cloned().unwrap_or_default()
            } else {
                Vec::new()
            };
            Some(DeckPackageMedia {
                media_asset_id: format!("anki-media-{}", normalize_slug(key)),
                original_filename: filename.clone(),
                mime_type: mime_type(filename).to_string(),
                bytes,
            })
        })
        .collect()
}

fn media_policy_blocks(
    manifest: &BTreeMap<String, String>,
    entries: &HashMap<String, Vec<u8>>,
) -> Vec<Value> {
    let mut missing = 0;
    let mut unsupported = 0;
    let mut audio_video = 0;
    for (key, filename) in manifest {
        if !entries.contains_key(key) {
            missing += 1;
        } else if is_audio_video(filename) {
            audio_video += 1;
        } else if !is_image(filename) {
            unsupported += 1;
        }
    }
    let mut blocks = Vec::new();
    if missing > 0 {
        blocks.push(json!({"code": "missing_media", "severity": "warning", "count": missing}));
    }
    if unsupported > 0 {
        blocks.push(
            json!({"code": "unsupported_media", "severity": "warning", "count": unsupported}),
        );
    }
    if audio_video > 0 {
        blocks.push(json!({"code": "audio_video_media_stripped", "severity": "warning", "count": audio_video}));
    }
    blocks
}

fn top_deck_name(collection: &CollectionData) -> String {
    collection
        .decks
        .values()
        .find(|deck| deck.name.trim() != "Default")
        .or_else(|| collection.decks.values().next())
        .map(|deck| {
            deck.name
                .split("::")
                .last()
                .unwrap_or(&deck.name)
                .trim()
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Anki Cards".to_string())
}

fn is_image(filename: &str) -> bool {
    matches!(
        extension(filename).as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp"
    )
}

fn is_audio_video(filename: &str) -> bool {
    matches!(
        extension(filename).as_str(),
        "mp3" | "m4a" | "wav" | "ogg" | "oga" | "mp4" | "mov" | "webm"
    )
}

fn mime_type(filename: &str) -> &'static str {
    match extension(filename).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn extension(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn normalize_zip_name(name: &str) -> CliResult<String> {
    let normalized = name.replace('\\', "/").trim().to_string();
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains("/../")
        || normalized.starts_with("../")
        || normalized.ends_with("/..")
        || Path::new(&normalized).is_absolute()
    {
        return Err(CliError::Usage(
            "apkg contains an unsafe file path".to_string(),
        ));
    }
    Ok(normalized)
}

fn write_temp_collection(bytes: &[u8]) -> CliResult<PathBuf> {
    let path = std::env::temp_dir().join(format!(
        "granoflow-anki-collection-{}.anki2",
        Uuid::new_v4()
    ));
    fs::write(&path, bytes).map_err(|error| {
        CliError::Usage(format!("failed to write temp anki collection: {error}"))
    })?;
    Ok(path)
}

fn current_ms_for_rows() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

fn normalize_slug(value: &str) -> String {
    let result = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    result.trim_matches('-').to_string()
}

fn sql_error(error: rusqlite::Error) -> CliError {
    CliError::Usage(format!("failed to read anki sqlite collection: {error}"))
}
