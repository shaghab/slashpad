# CLAUDE.md — guidance for AI-assisted work on SlashPad

## Project
Native Windows 11 text editor in Rust. Tagline: *Less memory. More text.* Read
`docs/PRD.md`, `docs/ARCHITECTURE.md`, `docs/decisions/` before changing architecture.

## Non-negotiables
- Document integrity first: never truncate the original before the new content is durable;
  preserve untouched bytes exactly (ADR-0004, ADR-0006).
- UI thread never blocks on I/O, indexing, or search.
- No network access, telemetry, or async runtimes in the core application.
- `slashpad-text` and `slashpad-core` are platform-independent and `#![deny(unsafe_code)]`.
  All `unsafe` lives in `crates/slashpad/src/platform/` with `// SAFETY:` comments.
- No `unwrap()`/`expect()` on recoverable paths.
- Never claim a test passes or a performance target is met without running/measuring it;
  record measurements in `docs/BENCHMARKS.md` with commit and hardware.
- Do not commit generated large test files.
- Do not push or publish without the owner's approval.

## Workflow per change
`cargo fmt` → `cargo clippy --all-targets -- -D warnings` → `cargo test` →
release build → relevant benchmarks → update docs (and `docs/RUST_LEARNING.md` when a new Rust
concept is introduced) → summarise results, limitations, debt.

## Environment notes
Cloud sessions run on Linux: core crates build and test there; the Windows binary can be checked
with `cargo clippy --lib --target x86_64-pc-windows-msvc` but not linked or run (`--all-targets`
fails there: criterion's `alloca` dependency needs the MSVC C toolchain). Windows tests and
benchmarks run on `windows-latest` CI or the owner's machine.
