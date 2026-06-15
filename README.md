# Granoflow Local HTTP API Client

`granoflow` is a headless Rust client for the Granoflow Local HTTP API.
It is not the old business CLI, does not write SQLite/Drift data directly,
and does not run scenario, restore, report, login, screenshot, App run, or
App build orchestration. Those responsibilities stay with `scripts/anz` and
the running App service layer.

## Run Locally

```text
cargo run -- --help
cargo run -- help task create --json
cargo run -- --json config
cargo run -- --api-base-url http://127.0.0.1:56789 --json health
```

## Configuration

Precedence is fixed:

```text
flags > env > config.toml > defaults
```

Supported env vars:

```text
GRANOFLOW_API_BASE_URL
GRANOFLOW_API_TOKEN
GRANOFLOW_CONFIG
```

The default API base URL is `http://127.0.0.1:56789`.

## Command Families

```text
granoflow health --json
granoflow api version --json
granoflow api capabilities --json
granoflow task list --json
granoflow task create --input <file|-> --json
granoflow task complete --id <id> [--input <file|->] --json
granoflow project list --json
granoflow project create --input <file|-> --json
granoflow review day show --date <YYYY-MM-DD> --json
granoflow review day update --date <YYYY-MM-DD> --input <file|-> --json
granoflow review week show --week-start <YYYY-MM-DD> --json
granoflow review week update --week-start <YYYY-MM-DD> --input <file|-> --json
granoflow review week value --week-start <YYYY-MM-DD> --value-id <id> --input <file|-> --json
granoflow ai-agent tools --json
granoflow ai-agent task export --id <task-id> --json
granoflow ai-agent task validate --input <file|-> --json
granoflow ai-agent task import --input <file|-> --dry-run --json
granoflow ai-agent task import --input <file|-> --json
granoflow ai-agent task validate --input <file|-> --json
granoflow ai-agent task import --input <file|-> --dry-run --json
granoflow ai-agent task import --input <file|-> --json
granoflow card archive <card-id> --json
granoflow card unarchive <card-id> --json
granoflow card trash <card-id> --json
granoflow card unlink --task-id <task-id> --card-id <card-id> --json
granoflow card unlink-note --task-id <task-id> --note-id <note-id> --json
granoflow backup decrypt --input <encrypted.flow.grano> --output <plaintext.flow.grano> --secret-env <ENV> --json
granoflow backup encrypt --input <plaintext.flow.grano> --output <encrypted.flow.grano> --secret-file <path> --json
```

`backup decrypt` and `backup encrypt` are offline package conversion utilities.
They run before API configuration is loaded and do not call the App or Local
HTTP API. The secret must come from exactly one of `--secret-env` or
`--secret-file`; JSON and human output never include the secret. Plaintext
packages intentionally remove the backup keyring/envelope and include a privacy
warning because losing that file exposes private records.

## Verification

```text
scripts/quality.sh
```

The quality gate runs:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

CI runs the same gate on pull requests and `main` pushes. Formatting drift,
clippy warnings, and failing tests are treated as release blockers.

Install the optional local pre-push hook with:

```text
scripts/setup-hooks.sh
```

The hook runs `scripts/quality.sh` before every push.

Lint configuration lives in:

- `Cargo.toml` `[lints]` for Rust and clippy lint levels.
- `rustfmt.toml` for formatting policy.
- `clippy.toml` for complexity and API-shape thresholds.

## Package And Sync

Build the release binary and copy it to the parent Granoflow repo's stable
scripts path:

```text
scripts/package.sh
```

By default this writes:

```text
../scripts/granoflow-cli
```

`scripts/anz` in the parent repo can then call the latest copied binary with a
stable path. Override the sync destination with `GRANOFLOW_CLI_SYNC_PATH` when
needed. If this repository is checked out beside the parent Granoflow repo
instead of as a submodule, the package script falls back to the legacy
`../granoflow/scripts/granoflow-cli` destination.

## Release Smoke

Release artifact names use:

```text
granoflow-v<version>-aarch64-apple-darwin.tar.gz
granoflow-v<version>-x86_64-apple-darwin.tar.gz
granoflow-v<version>-x86_64-unknown-linux-gnu.tar.gz
granoflow-v<version>-x86_64-pc-windows-msvc.zip
```

Local macOS smoke should build both installed macOS targets:

```text
cargo build --target aarch64-apple-darwin
cargo build --target x86_64-apple-darwin
```

Linux and Windows target builds may require platform toolchains in addition to
`rustup target add`. Record unavailable linker or SDK evidence in release notes
instead of marking those targets passed.
