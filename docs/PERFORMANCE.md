# SlashPad — Performance Strategy

Performance is a feature. Every number we publish is measured on a release build, on documented
hardware, by a script that anyone can re-run. Targets are listed in [PRD.md §5](PRD.md#5-performance-requirements);
results live in [BENCHMARKS.md](BENCHMARKS.md).

## 1. Definitions

| Term | Definition | How measured |
|---|---|---|
| Private working set | Private pages resident in RAM | `PROCESS_MEMORY_COUNTERS_EX2::PrivateWorkingSetSize` (`GetProcessMemoryInfo`) |
| Private commit | Private bytes charged to the commit limit (resident or not) | `PrivateUsage` |
| Total working set | Private + shareable resident pages (DLLs, mapped files) | `WorkingSetSize` |
| Mapped-file pages | Resident pages backed by data files | `QueryWorkingSetEx` / VMMap |
| Peak | Maximum of the above during an operation | `PeakWorkingSetSize`; sampled private WS at 10 ms |
| Cold start | Process start after file-cache/standby flush (RAMMap *Empty Standby List*, admin) or after reboot | harness |
| Warm start | Second consecutive launch | harness |
| Time to first frame | `CreateProcess` → end of the first `WM_PAINT` that drew document content | in-app QPC mark reported via `--perf-report` |
| Time to first viewport (file) | Open command → first frame containing file text | in-app marks |
| Time to full index | Open → line index complete | in-app marks |
| Typing latency | `WM_KEYDOWN/WM_CHAR` receipt → `Present`/`EndDraw` return | in-app marks (internal); end-to-end validated with a high-speed camera or PresentMon on the reference machine |
| Idle CPU | Process CPU time over 60 s with window open and focused, caret blink disabled | `GetProcessTimes` delta; wakeups via ETW/WPR |

The headline memory number is **private working set**, because that is what grows with our own
allocations. We always also report private commit and total working set so nothing is hidden.

## 2. Instrumentation

- A `perf` Cargo feature compiles in QPC timestamp marks (`perf::mark("first_frame")`). Without the
  feature the macro expands to nothing — zero cost in shipped builds.
- `slashpad.exe --perf-report <pipe|file>` (feature-gated) writes marks and memory counters as JSON,
  then optionally exits (`--perf-exit-after first_frame`).
- `cargo xtask bench startup|memory|open|typing` drives the release exe N times (default 20, first
  run discarded for warm runs) and reports median, p95, min, max.

## 3. Benchmark suites

| Suite | Tool | Where it runs |
|---|---|---|
| Data-structure micro-benchmarks (`slashpad-text`, search, encodings) | `criterion` | any OS; CI on every PR (trend) |
| Startup & idle memory | `xtask bench startup/memory` | Windows reference machine; CI weekly (trend) |
| Large-file open/index/search (1 MB … 10 GB) | `xtask bench open` + generated files | reference machine (10 GB), CI up to 1 GB |
| Typing latency | `xtask bench typing` using `SendInput` | reference machine |
| Pathological inputs | single 1–5 GB line, 10 M tiny lines, all-CR file, invalid UTF-8, mixed EOL | reference machine |

## 4. Reference hardware

To be filled in when the owner's Windows machine is profiled (CPU, RAM, storage model,
Windows build, display scale, GPU/driver). CI runners (`windows-latest`) are noisy shared VMs:
their results are used **only for regression trends with generous thresholds** (e.g. > 25 %
slower than the rolling median fails the bench job), never as published numbers.

## 5. Rules

1. Never claim a target is met without a measurement recorded in BENCHMARKS.md, with commit hash.
2. If a target proves unrealistic, record the evidence and propose a measurable replacement; do
   not relax it silently.
3. Every milestone ends with a benchmark run and a BENCHMARKS.md entry.
4. Performance regressions are bugs and block release.
