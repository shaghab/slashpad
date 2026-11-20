# ADR-0007: Never lock user files; detect staleness

Status: Accepted · Date: 2026-11-20

## Context
File-backed documents read from the original file after open. Locking it would break log writers,
rotation and other editors; not locking it means it can change underneath us.

## Decision
Open with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`. Keep a fingerprint (file ID,
size, last-write time). Re-validate on window activation, before save, and on every chunk-cache
miss. If changed: Normal-mode docs are unaffected (content is in memory) and the user is offered
reload; file-backed docs become **stale** — unedited ones reload automatically from the new file;
edited ones keep the already-cached content, block save-over, and offer "Save As" or reload.
Tail mode uses `ReadDirectoryChangesW` and appends new bytes when the file grows; truncation
triggers a reload.

## Consequences
No user-visible locking. A dirty large document whose source changed can only be saved under a new
name, because its untouched regions are no longer recoverable. This is the honest limit of
zero-copy editing.
