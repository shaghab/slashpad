# SlashPad — Technical Risk Register

Likelihood (L) and impact (I): H / M / L. Reviewed at the end of every milestone.

| ID | Risk | L | I | Mitigation | Validate by |
|---|---|---|---|---|---|
| R-01 | Direct2D/D3D11 initialisation pushes idle private WS above 20 MB or cold start above 300 ms | M | H | Measure hello-window variants in M0; fallback renderer (DirectWrite on GDI bitmap); defer D2D device creation until first paint | M0 Windows prototype |
| R-02 | IME (CJK composition, candidate window placement, reconversion) in a custom editor is incomplete | H | H | Start with IMM32 (works through TSF's compatibility layer on Win11); test with Microsoft Japanese/Chinese/Korean IMEs; TSF `ITextStoreACP` only if IMM32 proves insufficient | M0 IME spike; M1 acceptance |
| R-03 | Accessibility: custom-drawn editor/tabs invisible to Narrator/NVDA | H | H | Basic UIA `TextPattern` in M6, full in M7; architecture keeps text queries available without the renderer | M6 Narrator test |
| R-04 | Persistent piece tree has subtle bugs (offset arithmetic, line counts, CRLF split across pieces) | M | H | Property tests vs. oracle from day one; fuzzing (`cargo fuzz`) of edit sequences; debug invariant checks | M0/M1 |
| R-05 | `ReplaceFileW` with our own open shared-delete handle, on NTFS/ReFS/SMB/FAT, behaves unexpectedly; file-backed docs lose their source | M | H | Prototype on NTFS + SMB share in M0; fall back to in-place save with backup; keep source handle semantics documented | M0 save spike |
| R-06 | External process modifies a file that a file-backed (Large) document reads from ⇒ silent corruption of displayed/saved text | M | H | Fingerprint check on activation, before save, and on every chunk-cache miss (size/last-write); mark document *stale* and block save-over until resolved (ADR-0007) | M4 tests |
| R-07 | DirectWrite performance on very long lines / huge scripts (e.g. 1 MB of Arabic) | H | M | Windowed layout for lines > 64 KB; layout cache; benchmarks with pathological files | M4 |
| R-08 | Regex/streaming search semantics across chunk boundaries (matches spanning chunks, `^`/`$`, lookaround not supported by `regex`) | M | M | Overlap window; documented max match length in Large mode; differential tests vs. whole-buffer search | M6 |
| R-09 | Encoding detection guesses wrong on short legacy files | H | M | Detection is a guess with a visible status-bar indicator and one-click reopen-with-encoding; never write without round-trip check | M5 |
| R-10 | WebView2 runtime absent (Win11 includes it, but LTSC/stripped images may not), heavy process tree (100 MB+) | M | M | Lazy creation; graceful "preview unavailable"; native Markdown renderer as an alternative (D-10) | M6 |
| R-11 | Dark-mode menus/scrollbars require undocumented `uxtheme` ordinals | H | L | Owner-draw menus or a custom menu bar if ordinals break; isolate behind one module | M7 |
| R-12 | Development happens in a Linux container: Win32 code can be `cargo check`ed/clippy'd for `x86_64-pc-windows-msvc` but not linked, run, or benchmarked here | H | H | Core crates are cross-platform and tested here; Windows CI (`windows-latest`) runs tests; owner runs benchmarks/UI checks on a real Windows 11 machine | Ongoing |
| R-13 | Unsafe FFI code grows beyond the `platform` module, reducing safety and readability | M | M | `#![deny(unsafe_code)]` in `slashpad-text`/`slashpad-core`; `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks` lints in the bin | Clippy in CI |
| R-14 | Scope creep (P1/P2 lists are long) erodes performance budget | H | M | Performance budget checked at each milestone; P2 work only after P0 is stable | Milestone reviews |
| R-15 | Recovery journal itself becomes a corruption or privacy vector (sensitive text on disk) | M | M | CRC per record, versioned format, per-user ACLs, journals deleted on save/close; document in SECURITY.md; setting to disable | M5 |
| R-16 | Cold-start dominated by things we do not control (Defender scanning a new exe, DirectWrite font cache service) | M | M | Measure separately; avoid loading `shell32`/`ole32` until needed; sign releases | M1, M7 |
| R-17 | ARM64 later: SIMD paths or D3D behaviour differ | L | L | Only portable crates (`memchr` has NEON); add ARM64 build to CI early, even without tests | M7 |
