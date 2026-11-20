# SlashPad — Testing Strategy

## Layers

| Layer | Scope | Tooling | Runs on |
|---|---|---|---|
| Unit | functions/modules in every crate | `#[test]` | Linux + Windows CI |
| Property | piece tree: random edit sequences vs. oracle (`Vec<u8>` and `ropey`); line counts; snapshots unaffected by later edits; undo/redo round trips | `proptest` | Linux + Windows CI |
| Fuzz | buffer edit scripts, encoding detection/decoding, journal parsing, settings parsing | `cargo fuzz` (nightly, local/weekly) | Linux |
| Integration (core) | open/save round-trips for every encoding × EOL × BOM; atomic save failure injection; recovery replay; external modification; split-view consistency (two views, interleaved edits, undo) | `tests/` in `slashpad-core` with temp dirs | Linux + Windows |
| Platform integration | `ReplaceFileW`, file IDs, sharing modes, read-only/locked files, long paths, SMB share (manual) | `tests/` gated `cfg(windows)` | Windows CI |
| UI smoke | launch exe, open file via CLI, type via `SendInput`, save, compare bytes; later UIA queries | `xtask ui-smoke` | Windows (reference machine; CI where stable) |
| Benchmarks | see PERFORMANCE.md | `criterion`, `xtask bench` | as documented |

## Required test themes (from the brief)
File operations · Unicode & encodings · line-ending preservation (including mixed and lone CR) ·
undo/redo · split-view consistency · search & replace · recovery · external modification ·
performance · memory.

## Integrity tests (must never regress)
- **Byte-exact round trip**: open → save without edits ⇒ identical bytes, for every corpus file,
  including invalid UTF-8, mixed EOL, no trailing newline, BOM variants, empty file.
- **Failed save leaves original intact**: inject failures at each step (temp create, write,
  flush, replace) ⇒ original unchanged, temp cleaned up, user informed.
- **Disk full**: write to a small VHD / quota-limited directory (Windows, manual or scripted).
- **Crash mid-journal-write**: truncated last record is ignored; earlier edits recovered.

## Synthetic test data
`cargo xtask gen-testfiles [--max 10G] [--out DIR]` generates deterministic files (seeded) of
1 KB, 1 MB, 10 MB, 100 MB, 500 MB, 1 GB, 5 GB, 10 GB in variants: plain text, log (CRLF), CSV,
JSON, mixed Unicode (CJK, Arabic, emoji with ZWJ sequences, combining marks), single giant line,
10 M empty lines, invalid UTF-8 sprinkled. Output defaults to `target/testdata/` (git-ignored).
CI generates up to 1 GB; larger sizes are for the reference machine.

A small hand-curated corpus of tiny edge-case files (< 100 KB total) *is* committed under
`crates/slashpad-core/tests/corpus/`.
