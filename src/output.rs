use serde::Serialize;
use serde_json::{json, Value};

use crate::errors::CliError;

#[derive(Debug, Serialize)]
pub struct Envelope {
    pub ok: bool,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl Envelope {
    pub fn success(data: Value) -> Self {
        Self {
            ok: true,
            code: "ok".to_string(),
            data: Some(data),
            error: None,
            hint: None,
            meta: None,
        }
    }

    pub fn error(error: &CliError) -> Self {
        Self {
            ok: false,
            code: error.code().to_string(),
            data: None,
            error: Some(json!({"message": error.to_string()})),
            hint: None,
            meta: None,
        }
    }
}

pub fn print_json(envelope: &Envelope) -> i32 {
    println!(
        "{}",
        serde_json::to_string_pretty(envelope).expect("envelope serializes")
    );
    if envelope.ok {
        0
    } else {
        match envelope.code.as_str() {
            "usage_error" => 2,
            "config_error" => 3,
            "auth_error" => 4,
            "network_error" => 5,
            "api_error" => 6,
            "unsupported_feature" | "api_gap" => 7,
            _ => 10,
        }
    }
}

pub fn redact_token(value: &str) -> String {
    if value.is_empty() {
        return "".to_string();
    }
    if value.len() <= 6 {
        return "***".to_string();
    }
    format!("{}…{}", &value[..3], &value[value.len() - 3..])
}
