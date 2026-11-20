# ADR-0001: Win32 + `windows` crate + DirectWrite custom editor

Status: Proposed · Date: 2026-11-20

## Context
Requirements: idle private WS < 20 MB, cold start < 300 ms, native Windows behaviour, files up to
10 GB, split views over one buffer, full Unicode/IME, accessibility, Rust learning value.
See ARCHITECTURE §1 for the full comparison.

## Decision
Build directly on Win32 using Microsoft's `windows` crate. Native menus, Common Item Dialogs and
`ChooseFont`; custom-drawn editor, tab bar, status bar and find bar rendered with DirectWrite via
Direct2D. The renderer sits behind a small internal trait so a DirectWrite-on-GDI-bitmap
backend can replace Direct2D if Milestone 0 shows D3D initialisation breaks the memory target.

## Consequences
+ Lowest achievable memory and startup; full control over large-file rendering.
+ Platform-independent core crates (buffer, model) are pure safe Rust.
− We must implement caret/selection, IME (IMM32 first) and UI Automation ourselves (R-02, R-03).
− Win32/COM code is `unsafe`; confined to `crates/slashpad/src/platform`.

## Alternatives
Scintilla (whole-file in memory, C++); RichEdit (fails large files and split views); egui/iced/
Slint/gpui (GPU frameworks, non-native, higher memory); WinUI 3 (no Rust projection, heavy runtime).
