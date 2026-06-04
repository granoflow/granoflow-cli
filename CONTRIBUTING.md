# Contributing

Thanks for helping improve the Granoflow Local HTTP API client.

## Scope

This crate is a headless client for the Granoflow Local HTTP API. It does not
write app databases directly and does not orchestrate app builds, screenshots,
restore flows, reports, or scenario runners.

## Quality Gate

Run the full local gate before opening a pull request:

```text
scripts/quality.sh
```

This runs:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
```

Pull requests should keep this gate green. Formatting drift, clippy warnings,
and failing tests are treated as release blockers.

The lint standards are checked from these files:

- `Cargo.toml` `[lints]`: forbids unsafe code and denies the default clippy lint set.
- `rustfmt.toml`: fixes the formatting profile.
- `clippy.toml`: fixes complexity, argument-count, and type-complexity thresholds.

## Command Changes

When adding or changing commands:

- Keep command handlers thin and delegate HTTP behavior to the API client.
- Preserve stable JSON envelopes.
- Add contract tests for user-visible command behavior.
- Update OpenAPI drift mapping when the command supports or intentionally skips
  a local API endpoint.
- Avoid logging API tokens or raw config secrets.

## Dependency Changes

Prefer small, well-maintained dependencies with clear licenses. Avoid adding
runtime dependencies for behavior that is already simple to express in the
standard library or existing crate set.
