# AGENTS

This repository has two important conventions that should be preserved across sessions: the Rust toolchain must be used through the local user installation instead of the sandbox rustup shim, and schema metadata is coordinated through the authoritative attribute registry.

## Rust toolchain policy

- Do not run bare `cargo` in restricted editor/sandbox sessions unless it is explicitly pointing at the user-installed toolchain binary.
- The default rustup shim often tries to write to `~/.rustup/settings.toml`, which is blocked in sandboxed chat/editor sessions and creates churn.
- Prefer the repo helper script: `./scripts/run-local-rust.sh ...`
- If a direct command is needed, set `CARGO_BIN` or invoke the actual toolchain binary from the user rustup install, for example a path under `~/.rustup/toolchains/<toolchain>/bin/cargo`.
- Keep the helper script and `.vscode/tasks.json` in sync with the local toolchain workaround.
- Treat rustup bootstrap / toolchain re-install churn as an environment issue, not as a repository bug.

## Attribute registry contract

- The registry in `rust/src/attribute_registry.rs` and `rust/config/attributes/boat-attribute-registry.yaml` is the source of truth for attribute metadata.
- When changing feature display names, display groups, or attribute metadata, update the registry and the generated schema behavior together.
- Avoid ad hoc string overrides that bypass the registry contract.
- Keep active compact summary fields separate from compatibility/deprecated values and from richer analytic payloads; do not mix those responsibilities.
- If an importer change affects window metrics, summary counts, or majority `group_id` handling, validate the behavior in the relevant importer tests.

## Working habits

- Reproduce the issue before fixing it.
- Prefer the smallest targeted change.
- Add or update regression tests for importer/schema bugs.
- Keep scope narrow; do not broaden attribute cleanup or index changes unless they are explicitly required by the issue.
- When in doubt, prefer the repo’s registry-driven design over local one-off overrides.
