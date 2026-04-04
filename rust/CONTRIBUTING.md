# Contributing

Thanks for contributing to Open Claude Code's Rust integration layer.

## Development setup

- Install the stable Rust toolchain.
- Work from `rust/`. If you started from the repository root, `cd rust/` first.
- Treat `claw-code-parity` as the upstream Rust core. Changes to `api`, `runtime`, `tools`, or `plugins` usually belong there first, then get pulled in here by bumping the pinned Git revision.

## Build

```bash
cargo build --workspace
cargo build --release --workspace
```

## Test and verify

Run the full Rust verification set before you open a pull request:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace
cargo test --workspace
```

If you change behavior, add or update the relevant tests in the same pull request.

## Code style

- Follow the existing patterns in the touched crate instead of introducing a new style.
- Format code with `rustfmt`.
- Keep `clippy` clean for the workspace targets you changed.
- Keep downstream compatibility shims small and document why they are needed when parity APIs change.
- Prefer focused diffs over drive-by refactors.

## Pull requests

- Branch from `main`.
- Keep each pull request scoped to one clear change.
- Explain the motivation, the implementation summary, and the verification you ran.
- Make sure local checks pass before requesting review.
- If review feedback changes behavior, rerun the relevant verification commands.
