# Open Claude Code Rust Workspace

This `rust/` directory is now the Rust integration layer for `open-claude-code`, not a standalone Rust CLI distribution.

The shared Rust core lives upstream in [`wangedoo518/claw-code-parity`](https://github.com/wangedoo518/claw-code-parity). This workspace keeps only the downstream product crates that integrate that core with Open Claude Code surfaces.

## Current crate layout

This workspace currently builds only:

- `desktop-core` - desktop-facing session, provider, and persistence integration
- `desktop-server` - HTTP server that exposes `desktop-core` to the shell
- `server` - lighter HTTP runtime surface used by Open Claude Code

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
