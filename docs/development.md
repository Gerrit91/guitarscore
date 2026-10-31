# Development Guide

This document provides guidelines for contributing to the GuitarScore project.

## Workspace Structure

The workspace follows a clear layering system:

1. **Layer 0 (L0)**: `gscore-model`
   - Document model wrapper
   - Query helpers and position types
   
2. **Layer 1 (L1)**: `gscore-io`, `gscore-layout`, `gscore-play`
   - File I/O operations
   - Layout engine for drawing operations
   - Playback system
   
3. **Layer 2 (L2)**: `gscore-edit`, `gscore-audio`
   - Command system and editing logic
   - Audio output implementations
   
4. **Layer 3 (L3)**: `gscore-session`
   - Session management
   - Recovery journaling
   
5. **Layer 4 (L4)**: `gscore-cli`, `gscore-app`
   - Command-line tools
   - GUI application shell

## Coding Standards

- Follow Rust idioms and best practices
- Prefer small, focused crates with well-defined APIs
- Avoid "big ball of mud" architecture
- Ensure all code compiles for `wasm32-unknown-unknown` target
- Never panic in non-test code

## Testing Requirements

All changes must pass:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo xtask layers`

Additionally, specific tests apply based on what's modified.