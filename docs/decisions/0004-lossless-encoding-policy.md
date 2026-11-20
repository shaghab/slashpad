# ADR-0004: Lossless encoding round-trip policy

Status: Proposed · Date: 2026-11-20

## Decision
1. UTF-8 files keep their raw bytes; invalid sequences are displayed as U+FFFD but preserved.
2. Other encodings are decoded to UTF-8 at open and verified by re-encoding. If the round trip is
   not exact, the document opens read-only with an explanation; the user may convert explicitly.
3. Saving text that the target encoding cannot represent is refused with an offer to save as UTF-8.
4. BOM presence and EOL style are document properties preserved on save.
5. UTF-16 encoding is implemented in-house (`encoding_rs` does not encode UTF-16).

## Consequences
Integrity over convenience. Differs from Notepad, which silently substitutes characters.
