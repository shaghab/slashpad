# Milestone 0 prototypes

Throwaway experiments that validate risky assumptions before the product code is written.
They are a separate Cargo workspace and are **not** product code (no lint gates, `unsafe` used
freely in the Windows spikes).

| Prototype | Question | Risk |
|---|---|---|
| `m0-scan` | How fast and how memory-hungry is line indexing of huge files (buffered, `mmap`, cold/unbuffered, rope)? | R-07, P-11/P-12 |
| `m0-win-render` | Startup time and idle memory of an empty window vs GDI vs Direct2D vs DirectWrite-on-GDI-bitmap | R-01, P-01–P-03 |
| `m0-win-save` | Does `ReplaceFileW` atomic save work with our own open handle, other handles, read-only files, hard links, alternate data streams, SMB/FAT? | R-05 |
| `m0-win-ime` | Is IMM32 enough for inline IME composition in a custom-drawn text area? | R-02 |

## Running the Windows spikes

Requirements: Windows 11, [rustup](https://rustup.rs) with the MSVC toolchain
(Visual Studio Build Tools "Desktop development with C++"), ~12 GB free disk for the 10 GB test.

```powershell
git clone https://github.com/shaghab/slashpad; cd slashpad
powershell -ExecutionPolicy Bypass -File prototypes\run-windows-spikes.ps1 -Ime
```

Optional parameters: `-Runs 10` (renderer repetitions), `-SizeGB 10`, `-DataDir D:\tmp`
(put the big file on the disk you want measured), `-SaveDir \\server\share\tmp` (also test saving
on a network share or a FAT/exFAT USB stick), `-SkipLarge`, `-KeepData`.

The script writes `prototypes\spike-results.txt`. Please paste that file back.

For the most representative numbers: plug in the laptop, close other heavy apps, and run once
without `-Ime` first so that build time does not overlap with measurements.

## IME manual checklist (`-Ime`)

Install the input methods first (Settings → Time & language → Language & region → Add a language):
Japanese, Chinese (Simplified, Microsoft Pinyin), optionally Korean. Switch with Win+Space.

1. **Japanese**: type `nihongo` → composition appears underlined at the caret; press Space for
   candidates (the list should appear just below the caret); Enter commits. The text must appear
   exactly once.
2. While composing, press Backspace: only the composition shrinks.
3. Press Esc during composition: nothing is inserted.
4. **Chinese Pinyin**: type `zhongwen`, pick a candidate with a number key.
5. **Korean** (if installed): type a few syllables; each syllable composes in place.
6. Start a composition, then drag the window: the candidate window should follow.

Record yes/no for each item in the checklist block appended to `spike-results.txt`.
