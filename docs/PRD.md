# SlashPad — Product Requirements Document

> **Less memory. More text.**
>
> Status: **Draft v0.2 (Milestone 0)** — refined from the original brief. Items marked
> **[DECISION]** need owner approval; see [decisions/README.md](decisions/README.md).

## 1. Vision

SlashPad is a native Windows 11 text editor that is as simple and fast as Notepad, adds a
small set of carefully chosen power features (tabs, split view, large files, previews), and
treats performance and document integrity as product features.

It is **not** an IDE, not a VS Code competitor, and not a platform for extensions.

## 2. Target users

| Persona | Typical need | What matters most |
|---|---|---|
| Everyday user | Notes, config edits, quick text | Instant start, familiar Notepad behaviour |
| Developer / admin | Logs, JSON, config, CSV | Large files, tail mode, search, encodings |
| Writer | Markdown drafts | Preview, word count, focus mode |
| Keyboard user / AT user | Everything without a mouse; screen readers | Shortcuts, UIA accessibility |

## 3. Product principles (ranked)

When principles conflict, the higher-ranked one wins.

1. **Document integrity.** Never lose or silently alter user data. Bytes not touched by the
   user are written back exactly as they were read.
2. **Responsiveness.** The UI thread never blocks on I/O, indexing or search.
3. **Low resource use.** Memory and CPU proportional to what is *visible and active*, not to
   file size or to features that are merely available.
4. **Simplicity.** Notepad-like defaults; advanced features are discoverable but out of the way.
5. **Native integration.** Windows 11 conventions for keyboard, DPI, theme, dialogs and
   accessibility.
6. **Privacy.** No network access, telemetry, ads, or accounts. Everything works offline.

## 4. Scope and priorities

Requirement IDs (`F-xxx`) are stable so tests and ADRs can reference them.

### P0 — Core product

**File operations**
- F-001 New, Open, Save, Save As (Common Item Dialog).
- F-002 Open via drag-and-drop onto the window and via command-line arguments
  (`slashpad.exe a.txt b.log`, plus `--line N` for jump-to-line).
- F-003 Multiple files open simultaneously (tabs).
- F-004 Detect external modification/deletion of open files; prompt to reload or keep.
  Clean (unmodified) documents may auto-reload (setting).
- F-005 Unsaved-change prompts on close tab / close window / exit / log-off (`WM_QUERYENDSESSION`).
- F-006 **Atomic save**: write to a temporary file in the same directory, flush, then replace.
  The original file is never truncated before the new content is durable (see ADR-0006).
- F-007 Preserve encoding, BOM and line endings (including *mixed* line endings) unless the
  user explicitly converts.

**Text editing**
- F-010 Insert/delete/select; caret movement by grapheme, word, line, page, document.
- F-011 Cut/copy/paste. The editor only ever inserts plain text (`CF_UNICODETEXT`).
- F-012 Multi-level undo/redo with sensible coalescing of typing.
- F-013 Find and replace (incremental, case-sensitive, whole word). Regex is P1.
- F-014 Word wrap toggle.
- F-015 Standard Windows shortcuts (Ctrl+N/O/S/Shift+S/W/Z/Y/X/C/V/A/F/H/G, Ctrl+Tab, etc.).
- F-016 Full Unicode text, including complex scripts, bidirectional text, emoji, and IME input
  (CJK). See §7 for the precise Unicode contract.
- F-017 Configurable font family and size.
- F-018 F5 inserts the local date/time (Notepad format by default: `h:mm tt M/d/yyyy` per locale).

**Tabs**
- F-020 New / close / reorder (drag) / keyboard navigation (Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1..9).
- F-021 Dirty indicator per tab.
- F-022 Session restore: open tabs, active tab, caret and scroll per view, and unsaved content.
  Tabs other than the active one are restored *lazily* (loaded on first activation).

**Split view**
- F-030 Horizontal or vertical split of one document into two views.
- F-031 One document buffer; views own only caret, selection and scroll.
- F-032 Edits appear in both views immediately; undo/redo is per document, not per view.
- F-033 Splitting allocates O(1) memory relative to document size.

**Large files**
- F-040 Open files from 0 bytes to ≥ 10 GB.
- F-041 First viewport is shown before the file is fully read or indexed.
- F-042 Search large files by streaming, with bounded memory.
- F-043 Loading, indexing, searching and saving never block the UI thread; all are cancellable.
- F-044 Adaptive document modes — Normal, Large, Extreme — with configurable thresholds
  (§6, ADR-0005).

**Recovery**
- F-050 Recover unsaved changes after a crash or power loss.
- F-051 Persist unsaved changes across normal restarts (Notepad-style "continue where you left off"),
  on by default, with a setting to disable.
- F-052 Recovery writes are proportional to the *edits*, never full copies of large documents.

**Encoding and line endings**
- F-060 UTF-8 (with/without BOM), UTF-16 LE/BE (with/without BOM), Windows legacy code pages
  (1250–1258, 874, 932/Shift_JIS, 936/GBK, 949, 950/Big5), ISO-8859-x, and the system ANSI code page.
- F-061 OEM code pages (437, 850, …) — **P1** (not covered by `encoding_rs`; needs Win32
  `MultiByteToWideChar` or a table crate).
- F-062 CRLF, LF and CR line endings; detection, display, and conversion.
- F-063 Detection with user override ("Reopen with encoding…", "Save with encoding…").
- F-064 **Lossless round-trip guarantee** — see §7.3 and ADR-0004.

### P1 — High priority
Markdown preview (side-by-side) · HTML preview (sandboxed, no scripts, no network by
default) · Ctrl+Shift+V paste as plain text (effectively the default; kept for muscle memory) ·
regex find/replace · Go to line · line numbers · configurable date format · recent files ·
reopen closed tab (Ctrl+Shift+T) · pin tabs · duplicate view · close others / to the right ·
status bar (line, column, encoding, EOL, selection length, file size) · basic syntax highlighting ·
read-only mode · tail mode · multiple windows · portable mode · OEM code pages ·
**basic UI Automation accessibility for the editor (moved up from P2 — see §9).**

### P2 — Medium priority
Light/dark/follow-system themes · focus mode · full screen · synchronized split scrolling ·
multiple cursors · column selection · bookmarks · file compare · search across tabs ·
line operations (sort, dedupe, trim) · case conversion · JSON format/minify/validate ·
counts · command palette · custom shortcuts · zoom.

> Note: light/dark theme following Windows is cheap and expected on Windows 11. It is
> recommended to implement *follow system theme* in Milestone 7 rather than treating it as P2.

### P3 — Low priority
Print/print preview (which also gives PDF via *Microsoft Print to PDF*) · Markdown → PDF ·
hex viewer · Base64/URL encode/decode · MD5/SHA-256.

> **Challenge:** a *plugin architecture* conflicts with "no unnecessary dependencies",
> "minimal memory", and "treat input as untrusted". It is removed from scope; revisit only after
> 1.0 with a concrete use case. **[DECISION D-15]**

### Non-goals
Language servers, debuggers, terminals, Git integration, project/folder trees, cloud sync,
collaborative editing, auto-update service running in the background.

## 5. Performance requirements

Targets, methodology and measurement definitions live in [PERFORMANCE.md](PERFORMANCE.md).
Summary of the refined targets (release build, reference hardware):

| ID | Metric | Target | Notes |
|---|---|---|---|
| P-01 | Cold start → first frame | < 300 ms | after standby-list flush |
| P-02 | Warm start → first frame | < 100 ms | |
| P-03 | Idle private working set, 1 empty tab | < 20 MB | **unverified; highest-risk target** (Direct2D/D3D) |
| P-04 | 20 small files open | < 50 MB private WS | |
| P-05 | 100 MB file → first viewport | < 1 s (aim < 150 ms) | |
| P-06 | 1 GB file → first viewport | < 2 s (aim < 200 ms) | lazy load makes this ~size-independent for UTF-8 |
| P-07 | Max viewable file | ≥ 10 GB | |
| P-08 | Typing latency, keystroke → `Present` | p99 < 16 ms; aim p99 < 8 ms internal | Normal mode |
| P-09 | Idle CPU | 0 wakeups/s with no timers active (caret blink excepted) | measured over 60 s |
| P-10 | Distribution size (core exe) | < 10 MB (expect 1–3 MB) | |
| **P-11 (new)** | Private WS while viewing a 10 GB file | < 100 MB, independent of file size | bounded cache + sparse index |
| **P-12 (new)** | Full line index of 1 GB (warm cache) | < 1 s | M0 prototype: ~0.26 s on Linux |
| **P-13 (new)** | Streaming literal search, 1 GB warm | < 1 s | |
| **P-14 (new)** | Idle WS with a preview pane open | reported separately | WebView2 runs out of process |

## 6. Document modes

| Mode | Default trigger | Storage | Features |
|---|---|---|---|
| Normal | < 32 MB | file read fully into memory | everything |
| Large | 32 MB – 2 GB | file-backed, bounded chunk cache, background line index | edit, bounded undo, highlighting off by default, approximate wrap |
| Extreme | ≥ 2 GB, or any line > 16 MB | file-backed | **read-only by default** (opt-in editing), streaming search, no highlighting |

Thresholds are settings and will be tuned from Milestone 4 benchmarks. **[DECISION D-06]**

## 7. Unicode and text-integrity contract

### 7.1 Units of text
The code base distinguishes four units and never mixes them implicitly:

| Unit | Rust representation | Used for |
|---|---|---|
| Byte offset | `ByteOffset(u64)` into the document's UTF-8 content | buffer, search, undo, anchors |
| Unicode scalar value | `char` | case mapping, regex |
| Grapheme cluster | via `unicode-segmentation` | caret movement, Backspace/Delete granularity, column display |
| Visual position | DirectWrite hit-testing (UTF-16 code units within a laid-out line) | rendering, mouse hit-testing, IME |

Conversions happen only at well-defined boundaries (e.g. UTF-8 ↔ UTF-16 within a single
visible line segment at render time).

### 7.2 Line endings
CRLF, LF and lone CR are all line breaks. They are stored as-is; nothing is normalised on load.
New line breaks typed by the user use the document's detected (dominant) style.

### 7.3 Lossless round-trip
- UTF-8 files are stored as raw bytes. **Invalid byte sequences are preserved** byte-for-byte and
  displayed as U+FFFD (or a hex marker). Saving an unedited region reproduces the original bytes.
- Non-UTF-8 files are decoded to UTF-8 at open. The decoder output is verified by re-encoding;
  if `encode(decode(bytes)) != bytes`, the document opens with a warning and is **read-only
  until the user confirms** a lossy save. **[DECISION D-04]**
- Saving characters that the target encoding cannot represent (e.g. emoji into Windows-1252)
  is blocked with a dialog offering UTF-8 instead. Never silently write `?`.

## 8. UI requirements
Title bar (standard DWM caption, Mica backdrop) · compact native menu bar · custom tab bar ·
editor · optional split divider · compact status bar. No ribbon, no permanent sidebar, no
welcome screen. Per-Monitor-V2 DPI awareness. Keyboard access to every command. Follows the
Windows light/dark setting (Milestone 7).

## 9. Accessibility
A custom-drawn editor is invisible to screen readers unless it implements UI Automation
(UIA) providers. This is a correctness requirement, not polish. A basic `TextPattern` provider
for the editor, and accessible names for the tab bar and status bar, are scheduled for
**Milestone 6**, with full support (Narrator text navigation, high contrast) in Milestone 7.
**[DECISION D-09]**

## 10. Security and privacy
Every file is untrusted. No execution of content. Previews: scripts disabled, all network
requests blocked, no navigation; `file://` subresources blocked by default. Bounded allocation
for every input-derived size. Recovery data lives under `%LOCALAPPDATA%\SlashPad` (per-user
ACL); in portable mode, next to the executable with a warning. No network access anywhere in
the core application. Dependencies audited with `cargo audit` / `cargo deny` in CI.

## 11. Known requirement conflicts

| # | Conflict | Resolution proposed |
|---|---|---|
| C1 | Idle < 20 MB vs. Direct2D (loads D3D11 + driver) | Measure in M0; fallback to DirectWrite on a GDI bitmap render target (no D3D) |
| C2 | Idle memory / "no background processes" vs. WebView2 preview | Preview is lazy, out-of-process, and measured separately (P-14); torn down when closed |
| C3 | 10 GB editing vs. integrity | Extreme mode read-only by default; editing relies on piece table + file fingerprinting |
| C4 | Don't lock users' files vs. file-backed large documents | Share read/write/delete; detect changes via fingerprint and treat document as *stale* (ADR-0007) |
| C5 | Correct shaping/bidi vs. multi-GB single lines | Lines > 64 KB are laid out in windows; shaping across window boundaries may be imperfect |
| C6 | "Native look" vs. Windows 11 tab bar | No native Win32 tab control looks like Win11; custom drawing ⇒ we own accessibility (see §9) |
| C7 | "Learning Rust" vs. Win32 FFI | FFI is unsafe-heavy; confined to one crate so the core is idiomatic safe Rust |
| C8 | Preserve encoding vs. typing unrepresentable characters | Block lossy save; offer UTF-8 |
| C9 | Session restore of many tabs vs. fast startup | Lazy tab restoration |
| C10 | Portable mode vs. protected recovery data | Portable recovery is stored next to the exe with a one-time warning |
