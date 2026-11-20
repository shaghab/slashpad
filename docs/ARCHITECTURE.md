# SlashPad — Architecture

Status: **Proposed (Milestone 0)**. Nothing here is final until the decisions in
[decisions/README.md](decisions/README.md) are approved.

Contents
1. [UI technology evaluation](#1-ui-technology-evaluation)
2. [Text-buffer evaluation](#2-text-buffer-evaluation)
3. [Recommended technology stack](#3-recommended-technology-stack)
4. [Application architecture](#4-application-architecture)
5. [Key subsystem designs](#5-key-subsystem-designs)
6. [Repository and source layout](#6-repository-and-source-layout)
7. [Dependencies](#7-dependencies)

Memory and startup figures below are **expectations to be verified in Milestone 0 on Windows**,
not measurements, unless explicitly labelled *measured*.

---

## 1. UI technology evaluation

Versions and licences checked against crates.io on 2026-11-20.

| Option | Memory / startup | Large files | Win integration | Text quality | IME / a11y | Footprint | Learning value | Verdict |
|---|---|---|---|---|---|---|---|---|
| **Win32 via `windows` 0.62 + Direct2D/DirectWrite, custom editor** (MIT/Apache) | Best achievable; only system DLLs | We control everything | Complete (menus, dialogs, DPI, DWM, jump lists) | DirectWrite = same engine as Windows | We implement IMM32/TSF and UIA ourselves | One crate (+ `windows-core`) | High: real systems Rust, with an unsafe boundary to design | **Recommended** |
| `windows-sys` raw bindings only | Same | Same | Same | Same | Same | Smaller compile | COM (DirectWrite) is painful without the `windows` wrappers | Use only for non-COM hot paths if ever |
| `winsafe` 0.0.29 / `native-windows-gui` 1.0 (MIT) | Good | n/a | Wraps common controls | GDI controls | Via controls | Moderate | Hides the platform you want to learn; NWG unmaintained since 2022; winsafe pre-1.0 | Not recommended |
| WinUI 3 / XAML | High (Windows App SDK runtime, XAML framework); slow cold start | Poor (XAML text controls) | True Win11 look | Good | Good | Requires runtime redistribution | No first-class Rust projection (`windows-ui-xaml` crate abandoned 2021) | Rejected |
| RichEdit (`msftedit.dll`) as editor | Good | Loads whole doc; poor at 100 MB+ | Native | Good (DirectWrite mode) | Built-in IME + a11y | None | Low (black box) | Rejected: fails large-file and split-view requirements |
| **Scintilla** (C++, Scintilla licence ≈ HPND) embedded | Good | Gap buffer, entire file in memory (≥ file size RAM; UTF-8 internal) | Native child window | DirectWrite backend | IME good; a11y limited | C++ build via `cc` | Low for Rust; editor engine is not ours | **Main alternative**; fails Extreme mode, would still need our own large-file viewer |
| `egui` 0.37 (MIT/Apache) | GPU context (wgpu/glow) — typically tens of MB idle; immediate mode redraws | Must build our own text widget anyway | Non-native look | Own rasteriser, no ClearType | AccessKit a11y; IME historically weak | Large tree | Good Rust, wrong product | Rejected |
| `iced` 0.14 (MIT) | wgpu; tens of MB | Own widget needed | Non-native | cosmic-text | IME improving; a11y limited | Large | Good Rust, Elm architecture | Rejected |
| `slint` 1.19 (GPL-3 / royalty-free / commercial) | Moderate | Own widget needed | Non-native | femtovg/skia | Partial | Large | DSL-centred | Rejected (licence complexity, footprint) |
| `gpui` 0.2 (Apache) | GPU (DirectX on Windows); built for Zed | Excellent text infra but Zed-specific | Non-native | Good | Early Windows support | Very large (5 MB crate, many deps) | Steep, unstable API | Rejected |
| `xilem` 0.4 / `floem` 0.2 | Unproven | — | — | — | — | — | Experimental | Rejected (immaturity) |

### Rendering sub-options (Win32 path)

| Option | Pros | Cons | Plan |
|---|---|---|---|
| **Direct2D `ID2D1HwndRenderTarget` + DirectWrite** | Simple, hardware accelerated, colour emoji, ClearType/grayscale AA | Loads D3D11 + GPU driver: idle memory cost is the biggest unknown for P-03 | Default candidate |
| Direct2D on a DXGI flip-model swap chain (D3D11 device) | Lowest latency, `DXGI_PRESENT` control, works with DirectComposition/Mica | More code; same D3D cost | Upgrade path if latency needs it |
| DirectWrite → `IDWriteBitmapRenderTarget` (GDI DIB), no D3D | Smallest memory; ClearType | CPU rasterisation; colour glyphs require manual `TranslateColorGlyphRun` | **Fallback if P-03 fails with Direct2D** |
| GDI / Uniscribe | Tiny | Legacy; worse shaping/emoji | Rejected |

**Recommendation (ADR-0001):** plain Win32 with the `windows` crate; a custom editor surface
rendered with DirectWrite via Direct2D, keeping the renderer behind a small internal trait so the
GDI-bitmap fallback can be swapped in if Milestone 0 measurements require it. Native Win32 menus,
Common Item Dialogs and `ChooseFont`; custom-drawn tab bar, status bar and find bar.

**Why not Scintilla?** It is the strongest "don't reinvent" argument, and it would deliver IME,
selection, and rendering in weeks. But (a) it stores the entire document in memory, so the
Large/Extreme modes — which are a headline feature — would need a second, separate viewer anyway;
(b) it moves the most interesting Rust work (the buffer engine) into C++; (c) accessibility is no
better than what we would build. The cost of our choice is real: IME, accessibility and text
selection are substantial work and are tracked as top risks (R-02, R-03).

---

## 2. Text-buffer evaluation

| Structure | Open cost | Insert/delete | Line lookup | Undo | Snapshots (for background search/save) | Multi-GB | Verdict |
|---|---|---|---|---|---|---|---|
| Contiguous `Vec<u8>` | O(n) read + RAM = n | O(n) | needs side index | full copies or op log | copy | ✗ | Rejected |
| Gap buffer | O(n) | O(1) amortised near gap; O(n) jumps | side index | op log | copy | ✗ | Rejected |
| Rope (`ropey` 1.6) | O(n), must be valid UTF-8; *measured* 1.8 s, 1.2 GB RSS for 1 GB | O(log n) | O(log n), built in | op log; cheap clones | O(1) clone (Arc nodes) | ✗ (RAM ≥ file) | Use as **test oracle**, not engine |
| Rope (`crop` 0.4) | Same class as ropey, faster in some benches | O(log n) | O(log n) | — | O(1) | ✗ | Same |
| Piece table (flat list of pieces) | O(1) — references original | O(pieces) | needs per-piece line counts | trivially by keeping old piece lists | copy of piece list | ✓ | Too slow after many edits |
| **Piece tree** (balanced tree of pieces, like VS Code) | O(1) + background indexing | O(log p) | O(log p) with per-node line-break counts | keep old roots | **O(1) with persistent (immutable, `Arc`-shared) nodes** | ✓ | **Recommended** |
| Memory-mapped file only | O(1) | read-only | side index | — | — | ✓ | Rejected as *the* buffer (see below), possible *source* for pieces |
| Chunked file with bounded cache | O(1) | read-only | sparse side index | — | — | ✓ | **Recommended source for large originals** |

### Why not memory-map the original file?
Measured on the prototype, `mmap` scanning is fastest (~8 GB/s warm) and its RSS is *file-backed*,
not private. However on Windows:
- A mapped file **cannot be truncated** by other processes (`ERROR_USER_MAPPED_FILE`), which breaks
  log rotation and tail mode.
- I/O errors on network drives or removable media surface as `EXCEPTION_IN_PAGE_ERROR` inside
  ordinary memory reads — effectively an unrecoverable crash in safe Rust.
- If another process truncates/rewrites the file, the mapped view changes underneath safe `&[u8]`
  references, which violates Rust's aliasing assumptions (undefined behaviour).

Positional reads (`ReadFile` with `OVERLAPPED` offsets / `FileExt::seek_read`) into a bounded LRU
chunk cache are nearly as fast (*measured* ~5 GB/s warm, 3 MiB RSS), fully safe, and degrade
gracefully. `mmap` remains an option to benchmark on Windows for read-only Extreme mode (ADR-0003).

### Recommended design (ADR-0002)

```
Document
 └─ TextBuffer (persistent piece tree, Arc-shared nodes)
     ├─ OriginalSource      — immutable bytes of the file as opened
     │     ├─ InMemory(Arc<[u8]>)          Normal mode
     │     └─ FileBacked(ChunkCache)       Large / Extreme mode (+ sparse line index)
     └─ AddBuffer           — append-only, chunked (Arc<[u8]> chunks, never reallocated)

Piece { source: Original | Add, start: u64, len: u32, line_breaks: u32* }
Node aggregates: total bytes, total line breaks (*Unknown until indexed for original ranges)
```

- **Content is UTF-8 bytes** (`[u8]`, not `str`): this is what lets us preserve invalid sequences
  byte-for-byte. Rendering decodes lossily; editing operations only ever insert valid UTF-8.
- **Persistence**: an edit creates O(log n) new nodes and returns a new root; old roots stay valid.
  This makes three hard features simple:
  - *Undo/redo* = a stack of roots (plus selections). Memory per entry is O(log n), not O(n).
  - *Background work* (search, save, recovery checkpoint) takes a `Snapshot` (an `Arc` clone of the
    root — O(1)) and runs on a worker thread with no locks.
  - *Split view*: views reference the same `Document`; nothing is duplicated.
- **Line breaks** are counted per piece for add-buffer pieces at insert time. For original ranges
  in file-backed mode, counts come from the background sparse line index (one checkpoint per 1 024
  lines ⇒ ~8 bytes per thousand lines; a 10 GB log of 100-byte lines needs ~800 KB). Until indexing
  reaches a region, its line numbers are *estimated* and shown as such.
- **Grapheme and visual positions** are never stored in the tree. They are computed for the
  visible region only.

Validation plan: a prototype in Milestone 0 with property tests (random edit sequences vs. a
`ropey`/`Vec<u8>` oracle) and benchmarks (insert/delete throughput, line lookup, memory per edit).
Only the newline-scanning half has been prototyped so far ([BENCHMARKS.md](BENCHMARKS.md)).

---

## 3. Recommended technology stack

| Concern | Choice |
|---|---|
| Language / toolchain | Rust stable, edition 2024, pinned via `rust-toolchain.toml`; MSVC target `x86_64-pc-windows-msvc` (later `aarch64-pc-windows-msvc`) |
| Windows API | `windows` crate (COM wrappers for DirectWrite/Direct2D/UIA/Shell), minimal feature set |
| Rendering | Direct2D + DirectWrite (fallback: DirectWrite on GDI bitmap) |
| Input / IME | Win32 messages; IMM32 composition first, TSF later if needed |
| Accessibility | UI Automation provider (`IRawElementProviderSimple`, `ITextProvider`) |
| Concurrency | UI thread + `std::thread` workers + `std::sync::mpsc`; wake the UI with `PostMessageW(WM_APP+n)`. **No async runtime.** |
| Text engine | In-house persistent piece tree (`slashpad-text`) |
| Encodings | `encoding_rs` (+ own UTF-16 encoder; `encoding_rs` does not encode UTF-16), `chardetng` for legacy detection |
| Search | `memchr::memmem` for literals; `regex-automata` for regex (P1) with chunk-streaming |
| Grapheme segmentation | `unicode-segmentation` |
| Settings | TOML via `serde` + `toml` **[DECISION D-08]** |
| Session / recovery | Small custom binary formats (versioned, checksummed) — no serde |
| Preview (P1) | `pulldown-cmark` → sanitised HTML (`ammonia`) → WebView2 via `webview2-com`, lazy **[DECISION D-10]** |
| Testing | built-in test harness, `proptest`, `criterion`, `ropey` (oracle) |
| Packaging | portable zip + MSIX or WiX MSI (Milestone 7) |

---

## 4. Application architecture

### 4.1 Layers and dependency direction

```
┌──────────────────────────────────────────────────────────────┐
│ slashpad (bin)            Windows app: windows, rendering,   │
│                           input/IME, UIA, menus, dialogs,    │
│                           theme, DPI, preview host           │
└───────────────┬──────────────────────────────────────────────┘
                │ uses (never the reverse)
┌───────────────▼──────────────────────────────────────────────┐
│ slashpad-core             Platform-independent editor model: │
│                           Workspace, Document, View, commands│
│                           undo groups, search jobs, session, │
│                           recovery journal, settings model,  │
│                           file I/O + encodings, layout-free  │
└───────────────┬──────────────────────────────────────────────┘
┌───────────────▼──────────────────────────────────────────────┐
│ slashpad-text             Piece tree, sources, line index,   │
│                           snapshots, cursors over bytes       │
└──────────────────────────────────────────────────────────────┘
```

Crates exist to *enforce* the direction at compile time: `slashpad-text` and `slashpad-core` cannot
depend on Win32, so they build and test on any OS (including the Linux CI used for fast feedback).
Platform-specific file operations (`ReplaceFileW`, file IDs) are isolated in `slashpad-core::fs::sys`
behind `cfg(windows)` with a portable fallback used only for tests.

### 4.2 Logical components

| Component | Lives in | Responsibility |
|---|---|---|
| Application lifecycle | bin `app` | `wWinMain`, single-instance policy, message loop, shutdown |
| Window management | bin `window` | top-level windows, layout of tab bar / panes / status bar, DPI |
| Workspace / document management | core `workspace` | owns all `Document`s and `View`s via generational IDs |
| Text buffer | text | piece tree, snapshots, line index |
| Editor state | core `view` | selections, caret affinity, scroll anchor, preferred column |
| Editor views (UI) | bin `editor` | hit-testing, painting, caret, IME, scroll bars |
| Text rendering | bin `render` | DirectWrite layouts, glyph cache, line-layout cache |
| File I/O & encodings | core `fs`, `encoding` | detection, decoding, atomic save, fingerprints |
| Large-file handling | text `source` + core `jobs` | chunk cache, background indexing |
| Search | core `search` | streaming literal/regex search on snapshots |
| Undo/redo | core `history` | root stack + selection state, coalescing |
| Commands | core `command` | `enum Command`, dispatch, enabled-state queries |
| Keyboard shortcuts | core `keymap` + bin accelerator glue | key chord → `Command` |
| Session | core `session` | open tabs, positions, lazy restore |
| Crash recovery | core `recovery` | append-only edit journal per document |
| Settings | core `settings` | defaults, load/validate/save, portable mode paths |
| Preview | bin `preview` (P1) | WebView2 host, created on first use |
| Platform integration | bin `platform` | the only place with `unsafe` FFI; RAII wrappers |

### 4.3 Ownership model

- The **UI thread owns the `Workspace`** (`Vec`-backed slot maps of `Document` and `View`,
  addressed by `DocumentId`/`ViewId` with generation counters). No `Rc<RefCell<…>>` graph:
  windows and panes hold IDs, not references. This sidesteps most borrow-checker fights and makes
  "view of a closed document" a checked lookup instead of a dangling pointer.
- **Worker threads never touch the `Workspace`.** They receive owned inputs (a `Snapshot`,
  a path, a cancellation token) and send owned results back on a channel. The only shared
  mutable state is `Arc<AtomicBool>` cancellation flags and the internally synchronised chunk cache.
- **Edits flow one way**: `Command` → `Document::apply(edit)` → new root + `EditDelta` →
  every `View` of that document adjusts its anchors from the delta → views invalidate the
  affected visible lines.

### 4.4 Threads

| Thread | Lifetime | Purpose |
|---|---|---|
| UI | process | messages, input, layout, painting |
| Job workers (0–N) | on demand, exit when idle | load/decode, line indexing, search, save, recovery flush |

No thread exists while idle with no pending jobs (P-09). External-change detection uses
re-validation on window activation and before save (cheap metadata comparison); a
`ReadDirectoryChangesW` watcher is started only for tail mode (ADR-0007).

---

## 5. Key subsystem designs

### 5.1 Opening a file
1. Open handle with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` (never lock users out).
2. Record a **fingerprint**: volume serial + file ID, size, last-write time.
3. Read the first 64 KB → BOM / heuristic encoding detection.
4. Size < Normal threshold: read fully on a worker, verify round-trip, publish document.
   Otherwise: create `FileBacked` source, publish immediately, **render the first viewport from
   the first chunk**, then index in the background with progress in the status bar.
5. Non-UTF-8 large files: transcode on a worker into a private temp UTF-8 file in the recovery
   directory and use that as the original source (viewport from the transcoded prefix).

### 5.2 Saving (ADR-0006)
1. Take a snapshot; on a worker, stream pieces → encoder → temp file in the *same directory*
   (`.~slashpad-<id>.tmp`, hidden).
2. `FlushFileBuffers`; then `ReplaceFileW` (preserves ACLs, attributes, creation time).
3. If the target directory is not writable or `ReplaceFileW` is unsupported (some network shares,
   FAT), fall back only after **explicit confirmation** to: back up original → write in place → flush.
4. Symlinks are resolved first so the link target is replaced, not the link. Hard links are broken
   by replacement — detected and warned (option to write in place).
5. File-backed documents: the open read handle (shared-delete) keeps the old content readable
   during replacement; after success the document is rebased onto the new file.
   **This sequence must be prototyped on NTFS in Milestone 0 (R-05).**

### 5.3 Crash recovery (ADR-0008)
Per dirty document, `%LOCALAPPDATA%\SlashPad\Recovery\<uuid>.journal`:
- Header: format version, original path, original fingerprint, encoding/EOL metadata.
- Records: `Insert { at, bytes }`, `Delete { at, len }`, `Checkpoint { piece list }`, each with CRC32.
- Appended on an idle debounce (~1 s after typing stops, plus on focus loss); compacted
  periodically. Writes are proportional to typed text.
- Normal-mode documents under a small size (default 8 MB) also store one copy of the original
  bytes on first edit, so recovery works even if the original file later changes or is deleted.
- On startup, journals are validated (CRC, fingerprint) and replayed into restored tabs.

### 5.4 Rendering and long lines
- Only visible lines are laid out. Line layouts (`IDWriteTextLayout`) are cached per
  (line start, document revision, wrap width, font) with an LRU bound.
- Lines longer than 64 KB are processed in windows: no-wrap mode lays out only the horizontal
  window being shown; wrap mode uses an approximate wrap (estimated advance widths) for rows not
  on screen, refined for rows on screen.
- The vertical scrollbar maps to lines when the index is complete and to bytes before that.

### 5.5 Search
- Runs on a snapshot in a worker; streams chunks with an overlap of `needle_len − 1` bytes.
- Results are delivered incrementally (first match ASAP; count continues in the background).
- Regex (P1): `regex-automata` over chunks; matches longer than the chunk overlap window (default
  1 MB) are not found in Large/Extreme mode — documented limitation.

---

## 6. Repository and source layout

```
slashpad/
├─ Cargo.toml                 # workspace, shared lints & release profile
├─ Cargo.lock
├─ rust-toolchain.toml
├─ deny.toml                  # cargo-deny: licences + advisories
├─ README.md  LICENSE  CHANGELOG.md  CONTRIBUTING.md  SECURITY.md  ROADMAP.md  CLAUDE.md
├─ .github/
│  ├─ workflows/ci.yml        # fmt, clippy, test (windows-latest + ubuntu for core crates)
│  ├─ workflows/bench.yml     # manual/weekly release benchmarks (trend only)
│  ├─ ISSUE_TEMPLATE/  pull_request_template.md
├─ crates/
│  ├─ slashpad-text/          # piece tree, sources, line index, snapshots
│  │  ├─ src/{lib.rs, tree.rs, piece.rs, source/, line_index.rs, cursor.rs}
│  │  ├─ tests/               # property tests vs. oracle
│  │  └─ benches/
│  ├─ slashpad-core/          # editor model, I/O, encodings, search, session, recovery
│  │  ├─ src/{workspace.rs, document.rs, view.rs, history.rs, command.rs, keymap.rs,
│  │  │       search/, fs/, encoding.rs, session.rs, recovery.rs, settings.rs, jobs.rs}
│  │  ├─ tests/
│  │  └─ benches/
│  └─ slashpad/               # the Windows application (bin)
│     ├─ build.rs             # manifest (PerMonitorV2, UTF-8 code page, Common Controls 6), icon
│     ├─ res/                 # slashpad.ico, app.manifest, slashpad.rc
│     └─ src/{main.rs, app.rs, window.rs, editor/, render/, tabbar.rs, statusbar.rs,
│             findbar.rs, menu.rs, dialogs.rs, uia/, preview/, platform/}
├─ tools/
│  └─ xtask/                  # cargo xtask gen-testfiles | bench | measure-startup
├─ prototypes/                # throwaway M0 experiments (excluded from workspace)
└─ docs/
   ├─ PRD.md  ARCHITECTURE.md  PERFORMANCE.md  BENCHMARKS.md  TESTING.md  RUST_LEARNING.md
   ├─ RISKS.md
   └─ decisions/              # ADRs
```

Generated test files go to `%SLASHPAD_TESTDATA%` (default `target/testdata`, git-ignored).

---

## 7. Dependencies

Principle: a dependency must remove substantial, correctness-critical work, be actively
maintained, and be MIT/Apache-compatible. Everything is checked by `cargo deny` in CI.

### Runtime

| Crate | Version | Licence | Why | Notes |
|---|---|---|---|---|
| `windows` (+ `windows-core`) | 0.62 | MIT OR Apache-2.0 | Official Microsoft projection; COM for DirectWrite/D2D/UIA/Shell | Enable only needed features to keep compile time down |
| `memchr` | 2.8 | MIT OR Unlicense | SIMD newline counting (`memchr_iter`, `memchr2`) and `memmem` search | Already a transitive dep of `regex` |
| `encoding_rs` | 0.8 | (MIT OR Apache-2.0) AND BSD-3 | WHATWG-correct legacy encodings, streaming decode | Doesn't encode UTF-16 (we do) and lacks OEM code pages |
| `chardetng` | 1.0 | MIT OR Apache-2.0 | Legacy-encoding detection (Firefox's) | Used only when no BOM and not valid UTF-8 |
| `unicode-segmentation` | 1.13 | MIT OR Apache-2.0 | Grapheme/word boundaries for caret movement | Alternative `icu_segmenter` is heavier |
| `serde` + `toml` | 1.0 / 1.2 | MIT OR Apache-2.0 | Human-editable settings file | **[D-08]** — could hand-roll a tiny INI instead |
| `regex-automata` / `regex` | 0.4 / 1.13 | MIT OR Apache-2.0 | Regex search (P1); DFA stepping enables chunk streaming | Added in Milestone 6 |
| `pulldown-cmark` | 0.13 | MIT | Markdown → HTML (P1) | Milestone 6 |
| `ammonia` | 4.2 | MIT OR Apache-2.0 | Sanitise HTML for preview (defence in depth) | Milestone 6; pulls `html5ever` — evaluate size |
| `webview2-com` | 0.39 | MIT | WebView2 COM bindings (P1) | Milestone 6, lazy-loaded |

### Build / dev only

| Crate | Licence | Why |
|---|---|---|
| `embed-resource` | MIT | Compile `.rc` (icon, manifest, version info) |
| `proptest` 1.12 | MIT OR Apache-2.0 | Property tests for the piece tree |
| `criterion` 0.8 | MIT OR Apache-2.0 | Statistical micro-benchmarks |
| `ropey` 1.6 | MIT | Reference oracle for property tests |

### Deliberately not used
`tokio`/`async-std` (no async I/O need), `crossbeam` (std `mpsc` suffices), `notify` (one Win32 API
call does the job), `syntect`/`tree-sitter` (size and startup; a small incremental lexer per language
is planned for P1 highlighting), `mimalloc` (measure first; often raises idle RSS), `anyhow` in
library crates (typed errors via plain enums), `slotmap` (a 50-line generational arena is a good
Rust exercise).
