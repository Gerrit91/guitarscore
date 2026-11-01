# Claude Configuration for GuitarScore

This document provides instructions for setting up Claude to work effectively with the GuitarScore repository.

## Development Environment Setup

1. Install Rust via rustup (https://rustup.rs). `rust-toolchain.toml` pins the version and
   makes rustup install `rustfmt`, `clippy` and the `wasm32-unknown-unknown` target.
   With a distribution Rust instead, install the wasm target yourself
   (Arch: `pacman -S rust-wasm`).

2. Nothing else to install: `cargo xtask` is a cargo alias (`.cargo/config.toml`) for the
   in-repo `xtask/` crate. `cargo xtask ci` runs the full definition of done.

## Workspace Access

The repository follows a standard Rust workspace structure with the following crates:

```
crates/
├── gscore-gp/
├── gscore-model/
├── gscore-io/
├── gscore-layout/
├── gscore-play/
├── gscore-edit/
├── gscore-audio/
├── gscore-session/
├── gscore-cli/
└── gscore-app/
```

## File Naming Conventions

- All internal crate modules use snake_case convention
- Public API items should be documented with rustdoc comments
- Test files are named `test_*.rs` or in `tests/` directory

## Contribution Guidelines

1. Follow the existing code style (clippy warnings must pass)
2. Do not commit files that break the workspace layers check
3. Prioritize stability over feature completeness
4. All new features must be covered by tests
5. Implement recovery mechanisms before implementing new features

## Branching Strategy

- `main` branch contains stable code (M0 in progress; see `docs/devlog.md`)
- Feature branches should be used for implementation
- Pull requests should include test coverage and validation with:
  - `cargo test --workspace`
  - `cargo clippy --workspace`
  - `cargo xtask layers`
