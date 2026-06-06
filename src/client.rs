use serde_json::{json, Value};

use crate::{
    config::RuntimeConfig,
    errors::{CliError, CliResult},
};

#[derive(Clone)]
pub struct ApiClient {
    config: RuntimeConfig,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(config: RuntimeConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn get(&self, path: &str) -> CliResult<Value> {
        self.send(reqwest::Method::GET, path, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> CliResult<Value> {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }

    pub async fn patch(&self, path: &str, body: Value) -> CliResult<Value> {
        self.send(reqwest::Method::PATCH, path, Some(body)).await
    }

    pub async fn put(&self, path: &str, body: Value) -> CliResult<Value> {
        self.send(reqwest::Method::PUT, path, Some(body)).await
    }

    pub async fn delete(&self, path: &str) -> CliResult<Value> {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    async fn send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> CliResult<Value> {
        let url = format!("{}{}", self.config.api_base_url, path);
        let mut request = self.http.request(method, &url);
        if let Some(token) = &self.config.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| CliError::Network(error.to_string()))?;
        let status = response.status();
        let value = response
            .json::<Value>()
            .await
            .map_err(|error| CliError::Api(format!("invalid JSON response: {error}")))?;
        if !status.is_success() {
            let code = value
                .pointer("/error/code")
                .and_then(Value::as_str)
                .unwrap_or("api_error");
            let message = format!("{code}: {value}");
            return match status.as_u16() {
                401 | 403 => Err(CliError::Auth(message)),
                404 => Err(CliError::ApiGap(message)),
                _ if code == "unsupported_feature" => Err(CliError::UnsupportedFeature(message)),
                _ if code == "api_gap" => Err(CliError::ApiGap(message)),
                _ => Err(CliError::Api(format!("HTTP {status}: {value}"))),
            };
        }
        Ok(value)
    }
}

pub fn request_preview(method: &str, path: &str, body: Value) -> Value {
    json!({
        "previewMode": "local_request_only",
        "method": method,
        "path": path,
        "body": body,
        "hint": "preview_not_supported: the write endpoint was not called",
    })
}
