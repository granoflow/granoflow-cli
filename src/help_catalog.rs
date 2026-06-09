use serde_json::{json, Value};

use crate::cli::Lang;
use crate::openapi_drift::CLI_KNOWN_PATHS;

pub fn human_help(lang: &Lang, command: &[String]) -> String {
    let subject = if command.is_empty() {
        "granoflow".to_string()
    } else {
        command.join(" ")
    };
    match lang.resolved() {
        "zh-CN" => format!(
            "{subject}\n用法: granoflow <command> [--json]\n说明: Granoflow 本地 HTTP API 客户端。写命令使用 --input <file|->，AI/自动化调用建议加 --json。"
        ),
        "zh-TW" => format!(
            "{subject}\n用法: granoflow <command> [--json]\n說明: Granoflow 本地 HTTP API 客戶端。寫入命令使用 --input <file|->，AI/自動化呼叫建議加 --json。"
        ),
        _ => format!(
            "{subject}\nUsage: granoflow <command> [--json]\nNotes: Local HTTP API client. Write commands use --input <file|->; AI and automation should pass --json."
        ),
    }
}

pub fn json_help(lang: &Lang, command: &[String]) -> Value {
    json!({
        "requestedLang": lang.requested(),
        "resolvedLang": lang.resolved(),
        "command": command,
        "globalOptions": ["--api-base-url", "--token", "--config", "--json", "--lang"],
        "commands": [
            "health",
            "api version",
            "api capabilities",
            "task list",
            "task create --input <file|->",
            "task complete --id <id> [--input <file|->]",
            "project list",
            "project create --input <file|->",
            "review day show --date <YYYY-MM-DD>",
            "review day update --date <YYYY-MM-DD> --input <file|->",
            "review week show --week-start <YYYY-MM-DD>",
            "review week update --week-start <YYYY-MM-DD> --input <file|->",
            "review week value --week-start <YYYY-MM-DD> --value-id <id> --input <file|->",
            "deck list",
            "deck show <deck-id>",
            "deck create --name <name> [--parent <deck-id>]",
            "deck delete <deck-id>",
            "deck cards <deck-id> [--include-children] [--archived|--active]",
            "deck import anki <path.apkg> --dry-run",
            "deck import anki <path.apkg> --confirm <dry-run-id> [--skip-cards-with-missing-media] [--strip-remote-media]",
            "ai-agent tools",
            "ai-agent task export --id <task-id>",
            "ai-agent task validate --input <file|->",
            "ai-agent task import --input <file|-> [--dry-run]"
        ],
        "exitCodes": {
            "0": "success",
            "2": "usage or input validation error",
            "3": "config error",
            "4": "auth or permission error",
            "5": "network or unavailable API",
            "6": "API returned business error",
            "7": "unsupported feature or API gap",
            "8": "partial success",
            "10": "internal error"
        },
        "cliKnownPaths": CLI_KNOWN_PATHS
    })
}
