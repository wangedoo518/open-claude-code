# Open Claude Code Rust Workspace

This `rust/` directory is now the Rust integration layer for `open-claude-code`, not a standalone Rust CLI distribution.

The shared Rust core lives upstream in [`wangedoo518/claw-code-parity`](https://github.com/wangedoo518/claw-code-parity). This workspace keeps only the downstream product crates that integrate that core with Open Claude Code surfaces.

## Current crate layout

This workspace currently builds:

- `desktop-core` - desktop-facing session, provider, and persistence integration
- `desktop-server` - HTTP server that exposes `desktop-core` to the shell
- `desktop-cli` - local CLI client for desktop-server debugging and automation
- `server` - lighter HTTP runtime surface used by Open Claude Code
- `wiki_store` - ClawWiki on-disk raw/wiki/inbox/schema storage primitives
- `wiki_ingest` - ingestion helpers for raw knowledge inputs
- `wiki_maintainer` - LLM-backed absorb/query maintainer logic
- `wiki_patrol` - patrol and quality-check scaffolding for later phases

The following core crates are consumed from `claw-code-parity` as pinned Git dependencies:

- `api`
- `runtime`
- `tools`
- `plugins`

Current upstream pin:

- Repo: `https://github.com/wangedoo518/claw-code-parity.git`
- Rev: `736069f1ab45a4e90703130188732b7e5ac13620`

## Working model

- Core runtime, tool, provider, and plugin changes should be made in `claw-code-parity` first.
- Open Claude Code should keep Rust changes scoped to downstream integration concerns unless there is a deliberate upstreaming plan.
- If you need the Rust CLI binary itself, build it from `claw-code-parity`, not from this repository.
- Local LLM provider fallback is configured via `.claw/providers.json`; the maintainer adapter searches from the process cwd upward so a repository-root config still works when the desktop server starts from a nested app directory.
- `desktop-server` route assembly is split under `crates/desktop-server/src/routes/`
  (`desktop`, `wiki`, `wechat`, `internal`). Handler bodies are being split by
  domain under `crates/desktop-server/src/handlers/`. Landed slices include
  `handlers/wiki_reports.rs` for Wiki cleanup/patrol/report/stat endpoints and
  `handlers/wiki_tasks.rs` for absorb/query task endpoints and absorb progress
  SSE, plus `handlers/provider_runtime.rs` for Codex runtime/auth and
  providers.json CRUD endpoints, plus `handlers/desktop_sessions.rs` for
  desktop/ask session lifecycle, source binding, session SSE, compaction, and
  permission forwarding, plus `handlers/desktop_utilities.rs` for desktop
  bootstrap/settings, scheduled/dispatch CRUD, attachments, skills, MCP debug,
  and permission-mode endpoints, plus `handlers/desktop_storage.rs` for storage
  migration, MarkItDown/WeChat fetch helpers, URL-ingest diagnostics, and
  environment doctor probes, plus `handlers/wiki_crud.rs` for raw/inbox/page
  CRUD, lineage, proposal, combined-merge, and inbox notification handlers.
  `lib.rs` now owns shared `AppState`, common response/error helpers,
  private-cloud-only broker routes, shutdown wiring, and top-level Router
  assembly.

## Build and verify

Work from this directory:

```bash
cd rust
```

Run the standard checks:

```bash
cargo check --workspace
cargo test --workspace
```

If you need binaries for local debugging:

```bash
cargo build --workspace
cargo build --release --workspace
```

## Updating the parity pin

1. Update the `git`/`rev` entries in [`Cargo.toml`](./Cargo.toml).
2. Refresh the lockfile with a Cargo build or check.
3. Re-run `cargo check --workspace` and `cargo test --workspace`.
4. Call out any API compatibility shims added in `desktop-core` or `server` during review.

## Historical notes

- The old vendored `claw-cli` workspace is no longer the active build target here.
- Historical release notes from the vendored phase remain in [`docs/releases/0.1.0.md`](./docs/releases/0.1.0.md).

## License

See the repository root for licensing details.
