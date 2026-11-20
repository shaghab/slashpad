# Architectural Decisions

ADRs use the format: Context · Decision · Consequences · Alternatives. Status is one of
*Proposed*, *Accepted*, *Superseded*.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-ui-stack-win32-directwrite.md) | Win32 + `windows` crate + DirectWrite custom editor | Accepted |
| [0002](0002-persistent-piece-tree.md) | Persistent piece tree over raw UTF-8 bytes | Accepted |
| [0003](0003-no-mmap-by-default.md) | Bounded chunk cache, not memory mapping, for file-backed sources | Accepted |
| [0004](0004-lossless-encoding-policy.md) | Lossless encoding round-trip policy | Accepted |
| [0005](0005-document-modes.md) | Normal / Large / Extreme document modes | Accepted |
| [0006](0006-atomic-save.md) | Atomic save via temp file + `ReplaceFileW` | Accepted |
| [0007](0007-file-sharing-and-staleness.md) | Never lock user files; detect staleness | Accepted |
| [0008](0008-recovery-journal.md) | Append-only recovery journal | Accepted |

## Owner decisions

All recommendations below were **approved by the owner on 2026-11-20**.

| # | Decision | Recommendation | Main alternative |
|---|---|---|---|
| D-01 | UI stack | Win32 + `windows` + Direct2D/DirectWrite, custom editor (ADR-0001) | Embed Scintilla (faster to build, but no Extreme mode, less Rust) |
| D-02 | Text buffer | Persistent piece tree, raw UTF-8 bytes (ADR-0002) | `ropey` (simpler, but whole file in RAM) |
| D-03 | Licence | **MIT OR Apache-2.0 dual licence** (Rust ecosystem norm; Apache adds an explicit patent grant; MIT keeps GPLv2 compatibility) | Keep current MIT only (simplest; already in repo) |
| D-04 | Invalid bytes / lossy decodes | Preserve invalid UTF-8; non-round-trippable decodes open read-only until confirmed (ADR-0004) | Notepad behaviour: replace with U+FFFD silently |
| D-05 | File sharing | Share read/write/delete; detect staleness (ADR-0007) | Deny writes while a Large doc is dirty (safer, but blocks other apps) |
| D-06 | Mode thresholds | Normal < 32 MB; Extreme ≥ 2 GB or line > 16 MB; Extreme read-only by default (ADR-0005) | Editable everywhere |
| D-07 | Recovery | Edit journal; also copy originals < 8 MB on first edit (ADR-0008) | Periodic full snapshots (simpler, violates F-052 for large files) |
| D-08 | Settings format | TOML + `serde` (readable, standard) | Hand-written INI parser (fewer deps, ~200 LOC) |
| D-09 | Accessibility timing | Basic UIA in M6, full in M7 | Original plan: only in M7 polish |
| D-10 | Markdown preview | WebView2 for both Markdown and HTML (one sandboxed path) | Native DirectWrite Markdown renderer (lighter, more work); WebView2 only for HTML |
| D-11 | Process model for multiple windows | Single process, all windows on one UI thread | Process per window (isolation, more memory) |
| D-12 | Minimum OS | Windows 11 (build 22000+) only; Windows 10 is out of support | Also Windows 10 22H2 |
| D-13 | Toolchain | Stable MSVC target, edition 2024, MSRV = latest stable at milestone start, pinned | MinGW/`gnullvm` targets |
| D-14 | Where Windows work runs | Owner's Windows 11 PC for UI checks and benchmarks; `windows-latest` CI for tests; this cloud container for core crates and `cargo check --target x86_64-pc-windows-msvc` | — |
| D-15 | Plugin architecture (P3) | Remove from scope | Keep as P3 |
| D-16 | Status-bar column | Grapheme count from line start, 1-based (tabs count as 1; option for display column) | UTF-16 units (what Notepad uses internally) |
