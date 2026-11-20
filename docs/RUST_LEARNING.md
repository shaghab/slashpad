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

### `Arc`, `Arc::make_mut` and copy-on-write (piece tree)
`Arc<T>` is an atomically reference-counted shared pointer (like `std::shared_ptr<const T>`).
It only gives shared (`&T`) access — Rust forbids mutation through shared pointers. 
`Arc::make_mut(&mut arc)` returns `&mut T`: if this `Arc` is the only owner it mutates in place;
otherwise it clones the value first and repoints this `Arc` at the clone. The tree's edit
functions call it on every node along the edited path, which yields copy-on-write persistence
for free: versions kept by undo or by a background worker never see later edits.
`Rc` is the non-atomic variant; we use `Arc` because snapshots cross threads (`Send + Sync`).

### `Arc<[u8]>`: a shared slice with no separate length allocation
`Arc<[u8]>` is a *fat pointer* (pointer + length, like `std::span` plus a refcount). Converting a
`Vec<u8>` into it copies once, because the refcount must sit in front of the bytes.

### `cfg(test)` constants
`#[cfg(test)] const MAX_ENTRIES: usize = 4;` replaces the production value only when the crate's
own unit tests are compiled, so small inputs build deep trees and exercise every split/merge path.
Integration tests in `tests/` link the normal library build and therefore see production values —
we use both.

### Borrowing iterators (`Chunks<'a>`)
`Chunks<'a>` yields `&'a [u8]` slices pointing straight into the tree. The lifetime `'a` ties
them to the borrowed `TextBuffer`, so the compiler rejects any edit while the iterator (or a
slice from it) is alive — the iterator-invalidation bug class from C++ becomes a compile error.
To read while editing, clone the buffer (O(1)) and iterate the clone.

### Property-based testing with `proptest`
Instead of hand-picked cases, tests describe *strategies* for generating inputs (random text
biased towards `\r`, `\n` and UTF-8 fragments; random edit scripts) and assert that the tree
always agrees with a trivially correct model (`Vec<u8>`). Failures are automatically shrunk to a
minimal reproducing case. As a check, deliberately breaking the CRLF-junction logic made the
property tests fail immediately.

### Monoids
`Summary::combine` is associative with an identity (the empty summary). That algebraic property
is what lets any subtree cache its summary and lets queries combine cached summaries in any
grouping — and it is how a CRLF split across two pieces is counted once.
