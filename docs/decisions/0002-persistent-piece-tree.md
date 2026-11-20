# ADR-0002: Persistent piece tree over raw UTF-8 bytes

Status: Proposed · Date: 2026-11-20

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
