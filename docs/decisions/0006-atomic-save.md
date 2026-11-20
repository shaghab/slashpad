# ADR-0006: Atomic save via temp file + ReplaceFileW

Status: Accepted · Date: 2026-11-20

## Decision
Write the snapshot to a hidden temp file in the destination directory, `FlushFileBuffers`, then
`ReplaceFileW` (keeps ACLs, attributes, creation time, and the original stays intact on failure).
Resolve symlinks to their target first. If replacement is impossible (no directory write access,
unsupported file system), ask before falling back to backup-then-write-in-place. Hard links are
broken by replacement: detect (`nNumberOfLinks > 1`) and ask.

## Consequences
Saves need free space for one extra copy. Failure at any step leaves the original untouched.
Must be validated with an open shared-delete read handle on NTFS and SMB (R-05).
