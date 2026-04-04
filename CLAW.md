# CLAW.md

This file provides guidance to Claw Code when working with code in this repository.

## Detected stack
- Languages: TypeScript/TSX and Rust.
- Frameworks: React, Electron, Tauri, Bun/Vite.

## Verification
- Run Rust verification from `rust/`: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
- When touching the desktop shell, also validate the affected app flow in `apps/desktop-shell`.

## Repository shape
- `rust/` contains the downstream Rust integration workspace for `desktop-core`, `desktop-server`, and `server`.
- The shared Rust core is consumed from `claw-code-parity` via pinned Git dependencies in `rust/Cargo.toml`.
- `apps/desktop-shell/` is the only product frontend and talks to the Rust services.
- Historical duplicate Python mirror code has been removed; do not recreate local shadow implementations of parity-owned core logic.
- Do not reintroduce a parallel `desktop-web` mirror frontend; focus product work on `desktop-shell`.

## Working agreement
- Prefer small, reviewable changes and keep generated bootstrap files aligned with actual repo workflows.
- Keep shared defaults in `.claw.json`; reserve `.claw/settings.local.json` for machine-local overrides.
- If a Rust change really belongs in the upstream core, prefer upstreaming it to `claw-code-parity` instead of rebuilding the vendored fork here.
- Do not overwrite existing `CLAW.md` content automatically; update it intentionally when repo workflows change.
