use std::{env, fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    cli::Cli,
    errors::{CliError, CliResult},
    output::redact_token,
};

const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:56789";

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    api_base_url: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct RuntimeConfig {
    pub api_base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub config_path: String,
    pub token_source: String,
    pub api_base_url_source: String,
}

impl RuntimeConfig {
    pub fn load(cli: &Cli) -> CliResult<Self> {
        let config_path = cli
            .config
            .clone()
            .or_else(|| env::var("GRANOFLOW_CONFIG").ok())
            .map(PathBuf::from)
            .unwrap_or_else(default_config_path);
        let file_config = load_file_config(&config_path)?;
        let (api_base_url, api_base_url_source) = if let Some(value) = cli.api_base_url.clone() {
            (value, "flag".to_string())
        } else if let Ok(value) = env::var("GRANOFLOW_API_BASE_URL") {
            (value, "env".to_string())
        } else if let Some(value) = file_config.api_base_url {
            (value, "config".to_string())
        } else {
            (DEFAULT_API_BASE_URL.to_string(), "default".to_string())
        };
        let (token, token_source) = if let Some(value) = cli.token.clone() {
            (Some(value), "flag".to_string())
        } else if let Ok(value) = env::var("GRANOFLOW_API_TOKEN") {
            (Some(value), "env".to_string())
        } else if let Some(value) = file_config.token {
            (Some(value), "config".to_string())
        } else {
            (None, "none".to_string())
        };
        Ok(Self {
            api_base_url: api_base_url.trim_end_matches('/').to_string(),
            token,
            config_path: config_path.display().to_string(),
            token_source,
            api_base_url_source,
        })
    }

    pub fn redacted_json(&self) -> serde_json::Value {
        serde_json::json!({
            "apiBaseUrl": self.api_base_url,
            "apiBaseUrlSource": self.api_base_url_source,
            "token": self.token.as_deref().map(redact_token),
            "tokenSource": self.token_source,
            "configPath": self.config_path,
        })
    }
}

fn load_file_config(path: &PathBuf) -> CliResult<FileConfig> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| CliError::Config(format!("failed to read {}: {error}", path.display())))?;
    toml::from_str(&raw)
        .map_err(|error| CliError::Config(format!("failed to parse {}: {error}", path.display())))
}

fn default_config_path() -> PathBuf {
    ProjectDirs::from("com", "granoflow", "granoflow")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}
