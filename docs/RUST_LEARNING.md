# Rust Learning Notes

A running log of Rust concepts as they first appear in SlashPad, written for an engineer coming
from Java, C and C++. Production code stays free of tutorial comments; explanations live here.

## Concepts planned per milestone

| Milestone | Concepts | Where they show up |
|---|---|---|
| M0 | Cargo workspaces, crates vs. modules, editions, `Result` & `?`, iterators (`memchr_iter().count()`) | `prototypes/m0-scan` |
| M0/M1 | Ownership & moves, borrowing (`&` / `&mut`), slices `&[u8]` vs. `&str`, newtypes (`ByteOffset(u64)`) | `slashpad-text` |
| M1 | `Arc` vs. `Rc` vs. `Box`, structural sharing, `Send`/`Sync`, why the tree is immutable | piece tree nodes |
| M1 | `unsafe`, FFI, RAII via `Drop` (C++ destructors without exceptions), COM interface lifetimes in `windows` | `platform/` |
| M1 | Typed errors with `enum` + `impl std::error::Error`; why not `unwrap()` | `slashpad-core::fs` |
| M2 | Generational arenas instead of `Rc<RefCell<…>>` object graphs; the borrow checker and "IDs, not pointers" | `workspace` |
| M2 | Traits and trait objects vs. generics (static vs. dynamic dispatch, like templates vs. virtual) | renderer backend trait |
| M4 | Threads, channels, `AtomicBool`, `Mutex` scope, why there is no async runtime | `jobs` |
| M4 | Lifetimes in iterators that borrow a snapshot (`Chunks<'a>`) | text cursors |
| M5 | Property testing mindset, `proptest` strategies | tests |
| M6 | Lazy initialisation (`OnceCell`/`LazyLock`), feature flags (`cfg(feature = "perf")`) | preview, perf |

## Entries

### Edition 2024 keyword: `gen`
In the M0 prototype a function named `gen` failed to compile: `gen` is reserved in Rust 2024 for
future generator blocks. Editions let the language add keywords without breaking old crates — each
crate opts into an edition in `Cargo.toml`, and crates of different editions link together freely.
(C++ analogy: `-std=c++20` per translation unit, but with guaranteed ABI/interop across them.)

### `Result<T, E>` and `?`
Rust has no exceptions. Fallible functions return `Result`; `?` returns early with the error
(converting it via `From`). Like checked exceptions, but values: they compose with iterators and
cost nothing when not taken. Panics exist but are reserved for bugs, not for "file not found".
