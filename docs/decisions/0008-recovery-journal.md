# ADR-0008: Append-only recovery journal

Status: Accepted · Date: 2026-11-20

## Decision
Each dirty document has `%LOCALAPPDATA%\SlashPad\Recovery\<uuid>.journal` (portable mode: next to
the exe): versioned header (path, fingerprint, encoding, EOL), then CRC-protected records
`Insert{at, bytes}`, `Delete{at, len}`, `Checkpoint{pieces}`. Flushed ~1 s after typing stops and
on focus loss/session end; compacted when the journal exceeds a multiple of the edit volume. For
Normal-mode files below 8 MB, the original bytes are copied into the journal on first edit so
recovery doesn't depend on the original file. Journal deleted on save or discard.

## Consequences
Disk writes are proportional to edits (F-052). Recovery of a large edited document requires its
original to be unchanged (fingerprint-checked); otherwise only the edits' text can be offered.
