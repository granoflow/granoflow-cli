use std::{
    collections::HashMap,
    env, fs,
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::{
    cli::{BackupCommand, BackupConvertArgs, BackupSubcommand},
    errors::{CliError, CliResult},
};

const FORMAT_VERSION_V3: i64 = 3;
const NONCE_SIZE: usize = 12;
const KEY_SIZE: usize = 32;
const TAG_SIZE: usize = 16;
const KDF_ITERATIONS: u32 = 2;
const KDF_MEMORY_KIB: u32 = 65_536;
const KDF_PARALLELISM: u32 = 1;
const PRIVACY_WARNING: &str =
    "Plaintext Granoflow backup packages expose private records if this file is lost.";

pub fn run_backup(command: &BackupCommand) -> CliResult<Value> {
    match &command.command {
        BackupSubcommand::Decrypt(args) => convert_decrypt(args),
        BackupSubcommand::Encrypt(args) => convert_encrypt(args),
    }
}

fn convert_decrypt(args: &BackupConvertArgs) -> CliResult<Value> {
    let secret = read_secret(args)?;
    let package = BackupPackage::read(&args.input)?;
    let mut manifest = package.manifest()?;
    let format_version = manifest_format_version(&manifest)?;
    if format_version != FORMAT_VERSION_V3 {
        return Err(CliError::UnsupportedFeature(format!(
            "unsupported backup format_version {format_version}"
        )));
    }
    if package_kind(&manifest) == Some("plaintext") {
        return Err(CliError::Usage(
            "input package is already plaintext".to_string(),
        ));
    }
    if package_kind(&manifest) != Some("encrypted") {
        return Err(CliError::Usage(
            "backup decrypt requires package_kind=encrypted".to_string(),
        ));
    }
    let keyring = manifest_keyring(&manifest)?;
    let library_id = manifest_string(&manifest, "library_id")?;
    let dek = decrypt_dek(&secret.value, &library_id, keyring)?;
    let tables = manifest_tables(&manifest)?;
    let converted_entries = convert_record_entries(&package.entries, &tables, |table, row| {
        decrypt_record_row(&library_id, table, row, &dek)
    })?;

    set_common_v3_manifest_fields(&mut manifest, "plaintext")?;
    remove_object_key(&mut manifest, "keyring")?;
    manifest_set(&mut manifest, "privacy_warning", json!(PRIVACY_WARNING))?;
    manifest_set(
        &mut manifest,
        "cli_conversion",
        json!({"mode": "decrypt", "converted_at_ms": now_ms()?}),
    )?;

    BackupPackage {
        entries: converted_entries,
    }
    .write(&args.output, &manifest)?;

    Ok(json!({
        "command": "backup decrypt",
        "input": args.input,
        "output": args.output,
        "formatVersion": FORMAT_VERSION_V3,
        "packageKind": "plaintext",
        "secretSource": secret.source,
        "recordFiles": tables.len(),
        "privacyWarning": PRIVACY_WARNING
    }))
}

fn convert_encrypt(args: &BackupConvertArgs) -> CliResult<Value> {
    let secret = read_secret(args)?;
    let package = BackupPackage::read(&args.input)?;
    let mut manifest = package.manifest()?;
    if manifest_format_version(&manifest)? != FORMAT_VERSION_V3 {
        return Err(CliError::UnsupportedFeature(
            "backup encrypt requires plaintext format_version 3".to_string(),
        ));
    }
    if package_kind(&manifest) != Some("plaintext") {
        return Err(CliError::Usage(
            "backup encrypt requires package_kind=plaintext".to_string(),
        ));
    }
    if manifest.get("keyring").is_some() {
        return Err(CliError::Usage(
            "plaintext package must not contain keyring".to_string(),
        ));
    }

    let library_id = manifest_string(&manifest, "library_id")?;
    let dek = random_bytes(KEY_SIZE);
    let keyring = generate_keyring(&secret.value, &library_id, &dek)?;
    let tables = manifest_tables(&manifest)?;
    let converted_entries = convert_record_entries(&package.entries, &tables, |table, row| {
        encrypt_record_row(&library_id, table, row, &dek)
    })?;

    set_common_v3_manifest_fields(&mut manifest, "encrypted")?;
    manifest_set(&mut manifest, "keyring", keyring)?;
    remove_object_key(&mut manifest, "privacy_warning")?;
    manifest_set(
        &mut manifest,
        "cli_conversion",
        json!({"mode": "encrypt", "converted_at_ms": now_ms()?}),
    )?;

    BackupPackage {
        entries: converted_entries,
    }
    .write(&args.output, &manifest)?;

    Ok(json!({
        "command": "backup encrypt",
        "input": args.input,
        "output": args.output,
        "formatVersion": FORMAT_VERSION_V3,
        "packageKind": "encrypted",
        "secretSource": secret.source,
        "recordFiles": tables.len()
    }))
}

#[derive(Debug)]
struct SecretInput {
    value: String,
    source: &'static str,
}

fn read_secret(args: &BackupConvertArgs) -> CliResult<SecretInput> {
    match (&args.secret_env, &args.secret_file) {
        (Some(name), None) => {
            let value = env::var(name)
                .map_err(|_| CliError::Usage(format!("secret env var is not set: {name}")))?;
            require_non_empty_secret(value, "env")
        }
        (None, Some(path)) => {
            let value = fs::read_to_string(path)
                .map_err(|error| CliError::Usage(format!("failed to read secret file: {error}")))?;
            require_non_empty_secret(value.trim_end_matches(['\r', '\n']).to_string(), "file")
        }
        _ => Err(CliError::Usage(
            "exactly one of --secret-env or --secret-file is required".to_string(),
        )),
    }
}

fn require_non_empty_secret(value: String, source: &'static str) -> CliResult<SecretInput> {
    if value.trim().is_empty() {
        return Err(CliError::Usage("secret must not be empty".to_string()));
    }
    Ok(SecretInput { value, source })
}

#[derive(Debug)]
struct BackupPackage {
    entries: Vec<ZipEntryBytes>,
}

#[derive(Clone, Debug)]
struct ZipEntryBytes {
    name: String,
    bytes: Vec<u8>,
}

impl BackupPackage {
    fn read(path: &str) -> CliResult<Self> {
        let file = fs::File::open(path)
            .map_err(|error| CliError::Usage(format!("failed to open backup package: {error}")))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| CliError::Usage(format!("invalid backup zip: {error}")))?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .map_err(|error| CliError::Usage(format!("invalid backup entry: {error}")))?;
            if entry.is_dir() {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|error| {
                CliError::Usage(format!("failed to read backup entry: {error}"))
            })?;
            entries.push(ZipEntryBytes {
                name: normalize_zip_name(entry.name()),
                bytes,
            });
        }
        Ok(Self { entries })
    }

    fn manifest(&self) -> CliResult<Value> {
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.name == "manifest.json")
            .ok_or_else(|| CliError::Usage("missing manifest.json".to_string()))?;
        serde_json::from_slice(&entry.bytes)
            .map_err(|error| CliError::Usage(format!("manifest must be JSON: {error}")))
    }

    fn write(&self, output_path: &str, manifest: &Value) -> CliResult<()> {
        if Path::new(output_path).exists() {
            return Err(CliError::Usage(format!(
                "output package already exists: {output_path}"
            )));
        }
        let file = fs::File::create(output_path).map_err(|error| {
            CliError::Usage(format!("failed to create output package: {error}"))
        })?;
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file("manifest.json", options)
            .map_err(zip_error)?;
        writer
            .write_all(&serde_json::to_vec(manifest).map_err(internal_json_error)?)
            .map_err(io_error)?;
        for entry in self
            .entries
            .iter()
            .filter(|entry| entry.name != "manifest.json")
        {
            writer.start_file(&entry.name, options).map_err(zip_error)?;
            writer.write_all(&entry.bytes).map_err(io_error)?;
        }
        writer.finish().map_err(zip_error)?;
        Ok(())
    }
}

fn convert_record_entries(
    entries: &[ZipEntryBytes],
    tables: &[ManifestTable],
    convert: impl Fn(&str, &Value) -> CliResult<Value>,
) -> CliResult<Vec<ZipEntryBytes>> {
    let tables_by_path: HashMap<String, String> = tables
        .iter()
        .map(|table| (normalize_zip_name(&table.path), table.name.clone()))
        .collect();
    let mut converted = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(table_name) = tables_by_path.get(&entry.name) else {
            converted.push(entry.clone());
            continue;
        };
        let raw = String::from_utf8(entry.bytes.clone())
            .map_err(|error| CliError::Usage(format!("record file is not UTF-8: {error}")))?;
        let mut out = String::new();
        for line in raw.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let row: Value = serde_json::from_str(line)
                .map_err(|error| CliError::Usage(format!("record row must be JSON: {error}")))?;
            let converted_row = convert(table_name, &row)?;
            out.push_str(&serde_json::to_string(&converted_row).map_err(internal_json_error)?);
            out.push('\n');
        }
        converted.push(ZipEntryBytes {
            name: entry.name.clone(),
            bytes: out.into_bytes(),
        });
    }
    Ok(converted)
}

#[derive(Debug)]
struct ManifestTable {
    name: String,
    path: String,
}

fn manifest_tables(manifest: &Value) -> CliResult<Vec<ManifestTable>> {
    let tables = manifest
        .get("tables")
        .and_then(Value::as_array)
        .ok_or_else(|| CliError::Usage("manifest.tables must be an array".to_string()))?;
    tables
        .iter()
        .map(|table| {
            Ok(ManifestTable {
                name: table_value_string(table, "name")?,
                path: table_value_string(table, "path")?,
            })
        })
        .collect()
}

fn decrypt_record_row(library_id: &str, table: &str, row: &Value, dek: &[u8]) -> CliResult<Value> {
    let Some(payload) = row.get("payload").and_then(Value::as_str) else {
        return Ok(row.clone());
    };
    let field = CryptoField::decode(payload)?;
    let aad = backup_aad(library_id, field.version, table);
    let plaintext = aes_decrypt(
        dek,
        &field.nonce,
        &field.ciphertext,
        &field.tag,
        aad.as_bytes(),
    )?;
    serde_json::from_slice(&plaintext)
        .map_err(|error| CliError::Usage(format!("decrypted record is not JSON: {error}")))
}

fn encrypt_record_row(library_id: &str, table: &str, row: &Value, dek: &[u8]) -> CliResult<Value> {
    if row.get("payload").is_some() {
        return Err(CliError::Usage(
            "plaintext package contains encrypted payload row".to_string(),
        ));
    }
    let plaintext = serde_json::to_vec(row).map_err(internal_json_error)?;
    let field = aes_encrypt(dek, &plaintext, backup_aad(library_id, 1, table).as_bytes())?;
    Ok(json!({"payload": field.encode()}))
}

fn decrypt_dek(secret: &str, library_id: &str, keyring: &Value) -> CliResult<Vec<u8>> {
    let salt = b64_manifest_bytes(keyring, "kdf_salt_b64")?;
    let params_json = table_value_string(keyring, "kdf_params_json")?;
    let params: Value = serde_json::from_str(&params_json)
        .map_err(|error| CliError::Usage(format!("keyring kdf params invalid: {error}")))?;
    let umk = derive_umk(
        secret,
        &salt,
        json_u32(&params, "iterations")?,
        json_u32(&params, "memory")?,
        json_u32(&params, "parallelism")?,
    )?;
    let wrapped = b64_manifest_bytes(keyring, "wrapped_dek_b64")?;
    let nonce = b64_manifest_bytes(keyring, "wrapped_dek_nonce_b64")?;
    let tag = b64_manifest_bytes(keyring, "wrapped_dek_tag_b64")?;
    aes_decrypt(
        &umk,
        &nonce,
        &wrapped,
        &tag,
        envelope_aad(library_id).as_bytes(),
    )
    .map_err(|_| CliError::Auth("invalid backup secret".to_string()))
}

fn generate_keyring(secret: &str, library_id: &str, dek: &[u8]) -> CliResult<Value> {
    let salt = random_bytes(16);
    let umk = derive_umk(
        secret,
        &salt,
        KDF_ITERATIONS,
        KDF_MEMORY_KIB,
        KDF_PARALLELISM,
    )?;
    let wrapped = aes_encrypt(&umk, dek, envelope_aad(library_id).as_bytes())?;
    Ok(json!({
        "kdf_version": "argon2id_v1",
        "kdf_salt_b64": B64.encode(salt),
        "kdf_params_json": serde_json::to_string(&json!({
            "iterations": KDF_ITERATIONS,
            "memory": KDF_MEMORY_KIB,
            "parallelism": KDF_PARALLELISM
        })).map_err(internal_json_error)?,
        "wrapped_dek_b64": B64.encode(wrapped.ciphertext),
        "wrapped_dek_nonce_b64": B64.encode(wrapped.nonce),
        "wrapped_dek_tag_b64": B64.encode(wrapped.tag)
    }))
}

fn derive_umk(
    secret: &str,
    salt: &[u8],
    iterations: u32,
    memory: u32,
    parallelism: u32,
) -> CliResult<Vec<u8>> {
    let params = Params::new(memory, iterations, parallelism, Some(KEY_SIZE))
        .map_err(|error| CliError::Usage(format!("invalid argon2 params: {error}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = vec![0_u8; KEY_SIZE];
    argon
        .hash_password_into(secret.as_bytes(), salt, &mut key)
        .map_err(|error| CliError::Internal(format!("argon2 failed: {error}")))?;
    Ok(key)
}

#[derive(Debug)]
struct CryptoField {
    version: u32,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    tag: Vec<u8>,
}

impl CryptoField {
    fn decode(encoded: &str) -> CliResult<Self> {
        let parts: Vec<&str> = encoded.split(':').collect();
        if parts.len() != 4 || !parts[0].starts_with('v') {
            return Err(CliError::Usage(
                "invalid encrypted payload format".to_string(),
            ));
        }
        let version = parts[0][1..]
            .parse::<u32>()
            .map_err(|error| CliError::Usage(format!("invalid payload version: {error}")))?;
        Ok(Self {
            version,
            nonce: B64.decode(parts[1]).map_err(b64_error)?,
            ciphertext: B64.decode(parts[2]).map_err(b64_error)?,
            tag: B64.decode(parts[3]).map_err(b64_error)?,
        })
    }

    fn encode(&self) -> String {
        format!(
            "v{}:{}:{}:{}",
            self.version,
            B64.encode(&self.nonce),
            B64.encode(&self.ciphertext),
            B64.encode(&self.tag)
        )
    }
}

fn aes_encrypt(key: &[u8], plaintext: &[u8], aad: &[u8]) -> CliResult<CryptoField> {
    let nonce = random_bytes(NONCE_SIZE);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CliError::Internal("invalid AES key length".to_string()))?;
    let mut encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| CliError::Internal("AES-GCM encrypt failed".to_string()))?;
    if encrypted.len() < TAG_SIZE {
        return Err(CliError::Internal("AES-GCM output too short".to_string()));
    }
    let tag = encrypted.split_off(encrypted.len() - TAG_SIZE);
    Ok(CryptoField {
        version: 1,
        nonce,
        ciphertext: encrypted,
        tag,
    })
}

fn aes_decrypt(
    key: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
    aad: &[u8],
) -> CliResult<Vec<u8>> {
    if nonce.len() != NONCE_SIZE || tag.len() != TAG_SIZE {
        return Err(CliError::Usage("invalid AES-GCM field sizes".to_string()));
    }
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| CliError::Internal("invalid AES key length".to_string()))?;
    let mut input = Vec::with_capacity(ciphertext.len() + tag.len());
    input.extend_from_slice(ciphertext);
    input.extend_from_slice(tag);
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: &input, aad })
        .map_err(|_| CliError::Auth("backup decrypt failed".to_string()))
}

fn set_common_v3_manifest_fields(manifest: &mut Value, package_kind: &str) -> CliResult<()> {
    manifest_set(manifest, "format_version", json!(FORMAT_VERSION_V3))?;
    manifest_set(manifest, "package_kind", json!(package_kind))?;
    manifest_set(manifest, "backup_id", json!(Uuid::new_v4().to_string()))?;
    manifest_set(manifest, "created_at_ms", json!(now_ms()?))?;
    Ok(())
}

fn manifest_keyring(manifest: &Value) -> CliResult<&Value> {
    manifest
        .get("keyring")
        .ok_or_else(|| CliError::Usage("encrypted package missing keyring".to_string()))
}

fn manifest_format_version(manifest: &Value) -> CliResult<i64> {
    manifest
        .get("format_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| CliError::Usage("manifest.format_version is required".to_string()))
}

fn package_kind(manifest: &Value) -> Option<&str> {
    manifest.get("package_kind").and_then(Value::as_str)
}

fn manifest_string(manifest: &Value, key: &str) -> CliResult<String> {
    table_value_string(manifest, key)
}

fn table_value_string(value: &Value, key: &str) -> CliResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CliError::Usage(format!("{key} is required")))
}

fn manifest_set(manifest: &mut Value, key: &str, value: Value) -> CliResult<()> {
    let object = manifest
        .as_object_mut()
        .ok_or_else(|| CliError::Usage("manifest must be a JSON object".to_string()))?;
    object.insert(key.to_string(), value);
    Ok(())
}

fn remove_object_key(value: &mut Value, key: &str) -> CliResult<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| CliError::Usage("manifest must be a JSON object".to_string()))?;
    object.remove(key);
    Ok(())
}

fn b64_manifest_bytes(value: &Value, key: &str) -> CliResult<Vec<u8>> {
    let raw = table_value_string(value, key)?;
    B64.decode(raw).map_err(b64_error)
}

fn json_u32(value: &Value, key: &str) -> CliResult<u32> {
    let number = value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| CliError::Usage(format!("{key} must be a positive integer")))?;
    u32::try_from(number).map_err(|_| CliError::Usage(format!("{key} is too large")))
}

fn envelope_aad(library_id: &str) -> String {
    format!("{library_id}|dek|v1")
}

fn backup_aad(library_id: &str, version: u32, table: &str) -> String {
    format!("{library_id}|backup|v{version}|{table}")
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0_u8; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

fn now_ms() -> CliResult<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CliError::Internal(format!("system clock before epoch: {error}")))?;
    i64::try_from(duration.as_millis())
        .map_err(|_| CliError::Internal("timestamp overflow".to_string()))
}

fn normalize_zip_name(name: &str) -> String {
    name.replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn zip_error(error: zip::result::ZipError) -> CliError {
    CliError::Usage(format!("zip operation failed: {error}"))
}

fn io_error(error: std::io::Error) -> CliError {
    CliError::Usage(format!("I/O operation failed: {error}"))
}

fn b64_error(error: base64::DecodeError) -> CliError {
    CliError::Usage(format!("invalid base64 field: {error}"))
}

fn internal_json_error(error: serde_json::Error) -> CliError {
    CliError::Internal(format!("JSON serialization failed: {error}"))
}
