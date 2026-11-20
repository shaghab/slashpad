# ADR-0003: Bounded chunk cache, not memory mapping, for file-backed sources

Status: Accepted · Date: 2026-11-20

## Context
Large/Extreme documents read the original file lazily. Options: memory-map it, or positional
reads into a cache.

## Decision
Use positional reads (`seek_read`) of fixed-size chunks (default 1 MiB) into an LRU cache with a
byte budget (default 64 MiB). Memory mapping may be benchmarked later for read-only Extreme mode.

## Consequences
+ Safe Rust only; I/O errors are `Result`s, not `EXCEPTION_IN_PAGE_ERROR` crashes.
+ Doesn't prevent other processes from truncating the file (log rotation, tail mode).
+ Memory bounded and visible as our own private WS.
− Slightly slower than mmap (measured ~5 vs ~8 GB/s warm on Linux) — irrelevant at disk speeds.

## Alternatives
`memmap2` mapping: faster, but external truncation is UB for `&[u8]` views and blocks truncation.
