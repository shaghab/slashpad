# ADR-0002: Persistent piece tree over raw UTF-8 bytes

Status: Accepted · Date: 2026-11-20

## Context
Need O(1)-ish open for huge files, efficient edits, line lookup, cheap undo, split views,
background search/save without locking the UI, and byte-exact preservation of untouched content.

## Decision
`slashpad-text` implements a balanced tree of pieces referencing an immutable *original source*
(in-memory or file-backed) and an append-only *add buffer*. Nodes are immutable and shared via
`Arc`, so every edit yields a new root in O(log n) and old roots remain valid snapshots.
Content is `[u8]` assumed-but-not-required UTF-8; positions are `ByteOffset(u64)`. Node aggregates
hold byte length and line-break count (CR, LF, CRLF — with care for CRLF split across pieces).

## Consequences
+ Undo = stack of roots; snapshots for workers are O(1); split view shares one document.
+ Invalid UTF-8 survives untouched.
− `Arc` refcount traffic on edits (small; benchmark it). − More complex than a rope; mitigated by
property testing against `ropey`/`Vec<u8>` oracles and fuzzing.

## Alternatives
`ropey`/`crop` (require loading the whole file: measured 1.2 GB RSS for a 1 GB file), gap buffer,
flat piece table.

## Prototype findings (2026-11-20)
- Implemented in `crates/slashpad-text`; property tests and benchmarks in BENCHMARKS.md.
- No global add buffer: each inserted run owns an `Arc<[u8]>`; typing merges into the previous
  piece up to 1 KiB. Avoids `unsafe` (a shared, growing buffer would need it) and keeps memory
  reclamation automatic.
- In-memory pieces are capped at 16 KiB (16 KiB vs 64 KiB: line lookups 30–40 % faster, build and
  edit unchanged). File-backed sources (M4) will not be split into fixed pieces.
- Lookups are ~3–4× slower than `ropey` (≈2.7 µs vs ≈0.77 µs on 100 MiB) because the final step
  scans inside a piece; acceptable (viewport rendering needs one lookup per frame) and improvable
  with per-piece break offsets if profiling demands.
