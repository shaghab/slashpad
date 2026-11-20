# SlashPad — Roadmap and Implementation Plan

Each milestone ends with a compiling, tested application, a benchmark entry in
[docs/BENCHMARKS.md](docs/BENCHMARKS.md), updated docs, and a summary of limitations and debt.
Per-milestone workflow: acceptance criteria → design note → Rust concepts → smallest increment →
tests → `cargo fmt` / `clippy` / `test` → release build → benchmarks → docs → summary.

## Milestone 0 — Architecture and feasibility *(in progress)*
- [x] Inspect repository and environment
- [x] Evaluate UI technologies and text-buffer structures ([ARCHITECTURE](docs/ARCHITECTURE.md))
- [x] Large-file scanning prototype (`prototypes/m0-scan`) — results in BENCHMARKS
- [x] Risk register, performance methodology, testing strategy
- [x] **Owner approval of decisions D-01…D-16** (2026-11-20, all recommendations) ([decisions](docs/decisions/README.md))
- [x] Piece-tree prototype: insert/delete/line lookup + proptest vs. oracle + criterion benches
      (`crates/slashpad-text`)
- [ ] Windows spikes (need a Windows 11 machine or CI runner):
  - [ ] Hello window variants: empty Win32 / + Direct2D+DirectWrite / + DirectWrite-on-GDI-bitmap →
        cold/warm start, private WS, commit (decides R-01)
  - [ ] IMM32 composition in a custom-drawn control with Japanese & Chinese IMEs (R-02)
  - [ ] `ReplaceFileW` while holding a shared-delete read handle: NTFS, SMB share (R-05)
  - [ ] Cold-cache chunked read of 10 GB on NTFS
- **Exit:** approved ADRs; prototype measurements recorded; go/no-go on renderer.

## Milestone 1 — Minimal working editor
Cargo workspace (`slashpad-text`, `slashpad-core`, `slashpad`), CI, repository docs
(CONTRIBUTING, SECURITY, CHANGELOG, templates). One window, one document: caret, selection,
typing, Backspace/Delete by grapheme, mouse selection, clipboard (plain text), keyboard shortcuts,
open/save (UTF-8 + UTF-16 BOM; other encodings open read-only), atomic save, F5 date/time,
font setting, IMM32 IME, Per-Monitor-V2 DPI, `perf` instrumentation, startup/memory bench.
- **Acceptance:** byte-exact round trip on corpus; failed-save test leaves original intact;
  P-01/P-02/P-03 measured and reported (pass or documented gap).

## Milestone 2 — Document management
Workspace with generational IDs; tab bar (custom-drawn), new/close/switch/reorder; dirty
tracking (revision vs. saved revision); undo/redo as root stack with typing coalescing;
unsaved-changes prompts including `WM_QUERYENDSESSION`.
- **Acceptance:** 20-tab memory bench (P-04); undo/redo property tests.

## Milestone 3 — Split views
`View` separated from `Document`; anchors adjusted from `EditDelta`; horizontal/vertical split;
independent caret/selection/scroll; shared undo.
- **Acceptance:** split-view consistency tests; memory of split = O(1) (measured).

## Milestone 4 — Large-file engine
`FileBacked` source + bounded chunk cache; background sparse line index with progress and
cancellation; viewport-only rendering; long-line windowed layout; Large/Extreme modes; streaming
literal search; fingerprint/stale detection.
- **Acceptance:** P-05, P-06, P-07, P-11, P-12, P-13 measured with generated files up to 10 GB and
  pathological single-line files.

## Milestone 5 — Reliable daily-use editor
Find/replace UI (case, whole word); full encoding support with detection, override, round-trip
check and lossy-save protection; EOL detection/conversion; recovery journal + replay; session
restore (lazy tabs); external change detection and reload.
- **Acceptance:** kill-process recovery tests; encoding × EOL round-trip matrix.

## Milestone 6 — Productivity features (P1)
Regex search, Go to line, line numbers, status bar details, recent files, reopen closed tab,
pin/close-others/close-right, duplicate view, read-only mode, tail mode, multiple windows,
portable mode, OEM code pages, basic syntax highlighting, Markdown & HTML preview,
**basic UIA accessibility**.

## Milestone 7 — Polish and public release
Light/dark/follow-system theme, Mica, focus mode, full accessibility pass (Narrator, high contrast,
keyboard-only audit), performance tuning, ARM64 build, portable zip + installer, code signing
plan, documentation review, 1.0 release after owner approval.

## After 1.0 (P2/P3, not before P0 is stable)
See [docs/PRD.md](docs/PRD.md) §4.
