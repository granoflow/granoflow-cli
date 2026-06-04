# AGENTS

## Codex Entry

- Codex entering `granoflow-cli` must read this file first.
- This is an open-source Rust CLI project for the Granoflow Local HTTP API.
- Prefer project-owned commands over ad hoc tool invocations.

## Hard Rules

- Do not change the public CLI command surface unless the user explicitly asks.
- Do not add direct SQLite, Drift, App build, App run, screenshot, restore, report, or scenario orchestration logic.
- Do not print API tokens or config secrets in logs, tests, snapshots, or docs.
- Do not weaken tests, remove OpenAPI drift checks, or make quality gates non-blocking to get a green result.
- Do not create branches, commit, or push unless explicitly requested.

## Quality Gate

- Local quality gate: `scripts/quality.sh`.
- The gate must run:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-targets --all-features`
- Rust code changes should finish with `scripts/quality.sh`.
- If only a subset is run, the final response must explain why and name the residual risk.

## Refactor Standards

- Keep command parsing, API transport, config loading, command execution, output envelopes, and drift checks in separate modules.
- Command handlers should stay thin: parse args, read input, choose endpoint, and call the API client.
- Add helpers only when they name real CLI or API behavior; avoid thin wrappers created only to hide complexity.
- New command families require contract tests and OpenAPI drift mapping.
- Prefer explicit error types and stable JSON envelopes over ad hoc strings.
- Keep the crate free of `unsafe` code.

## Open Source Expectations

- README and CONTRIBUTING must show the same quality gate used by CI.
- CI must fail on formatting, clippy warnings, and tests.
- Public docs should describe supported behavior only; do not document internal Granoflow app workflows as CLI responsibilities.
