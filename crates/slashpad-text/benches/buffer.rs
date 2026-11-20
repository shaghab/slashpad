//! Micro-benchmarks for the piece tree, with `ropey` as a baseline where the
//! operations are comparable. Run with `cargo bench -p slashpad-text`.

#![allow(clippy::unwrap_used)] // benchmark setup: a failure should abort loudly

use std::hint::black_box;
use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use slashpad_text::TextBuffer;

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// UTF-8 log text of roughly `mb` MiB with CRLF line endings.
fn log_text(mb: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(mb << 20);
    let mut i = 0u64;
    while out.len() < mb << 20 {
        out.extend_from_slice(
            format!("2026-11-20T10:00:00Z INFO id={i} path=/api/v1/items ünïcödé ✓\r\n").as_bytes(),
        );
        i += 1;
    }
    out
}

fn build(c: &mut Criterion) {
    let mut g = c.benchmark_group("build");
    g.sample_size(10).measurement_time(Duration::from_secs(10));
    for mb in [10, 100] {
        let text = log_text(mb);
        g.throughput(Throughput::Bytes(text.len() as u64));
        g.bench_function(format!("piece_tree_{mb}MiB"), |b| {
            b.iter_batched(
                || text.clone(),
                |t| black_box(TextBuffer::from_bytes(t)),
                BatchSize::LargeInput,
            )
        });
        let s = String::from_utf8(text.clone()).unwrap();
        g.bench_function(format!("ropey_{mb}MiB"), |b| {
            b.iter(|| black_box(ropey::Rope::from_str(&s)))
        });
    }
}

fn edits(c: &mut Criterion) {
    let text = log_text(10);
    let mut g = c.benchmark_group("edit_10MiB");

    g.bench_function("piece_tree_insert_random", |b| {
        let mut buf = TextBuffer::from_bytes(text.clone());
        let mut rng = Rng(1);
        b.iter(|| {
            let at = rng.below(buf.len());
            buf.insert(at, b"x").unwrap();
        })
    });
    g.bench_function("ropey_insert_random", |b| {
        let mut rope = ropey::Rope::from_str(std::str::from_utf8(&text).unwrap());
        let mut rng = Rng(1);
        b.iter(|| {
            let at = rng.below(rope.len_chars());
            rope.insert_char(at, 'x');
        })
    });
    g.bench_function("piece_tree_insert_random_keep_snapshot", |b| {
        // Each edit happens while the previous version is alive (as with undo),
        // so the edit must copy its path instead of mutating in place.
        let mut buf = TextBuffer::from_bytes(text.clone());
        let mut rng = Rng(1);
        b.iter(|| {
            let snapshot = buf.clone();
            let at = rng.below(buf.len());
            buf.insert(at, b"x").unwrap();
            black_box(snapshot)
        })
    });
    g.bench_function("piece_tree_typing_sequential", |b| {
        let mut buf = TextBuffer::from_bytes(text.clone());
        let mut at = buf.len() / 2;
        b.iter(|| {
            buf.insert(at, b"k").unwrap();
            at += 1;
        })
    });
    g.bench_function("piece_tree_delete_random_16B", |b| {
        let mut buf = TextBuffer::from_bytes(text.clone());
        let mut rng = Rng(2);
        b.iter(|| {
            if buf.len() < 1 << 20 {
                buf = TextBuffer::from_bytes(text.clone());
            }
            let at = rng.below(buf.len() - 16);
            buf.delete(at..at + 16).unwrap();
        })
    });
    g.finish();
}

fn lookups(c: &mut Criterion) {
    let text = log_text(100);
    let mut buf = TextBuffer::from_bytes(text.clone());
    // Fragment the tree a little, as after an editing session.
    let mut rng = Rng(3);
    for _ in 0..10_000 {
        let at = rng.below(buf.len());
        buf.insert(at, b"edit\r\n").unwrap();
    }
    let rope = ropey::Rope::from_str(std::str::from_utf8(&text).unwrap());
    let lines = buf.line_count();
    let mut g = c.benchmark_group("lookup_100MiB");
    g.bench_function("piece_tree_line_start", |b| {
        let mut rng = Rng(4);
        b.iter(|| black_box(buf.line_start(rng.below(lines))))
    });
    g.bench_function("ropey_line_to_byte", |b| {
        let mut rng = Rng(4);
        let n = rope.len_lines();
        b.iter(|| black_box(rope.line_to_byte(rng.below(n))))
    });
    g.bench_function("piece_tree_line_of_offset", |b| {
        let mut rng = Rng(5);
        b.iter(|| black_box(buf.line_of_offset(rng.below(buf.len()))))
    });
    g.bench_function("ropey_byte_to_line", |b| {
        let mut rng = Rng(5);
        b.iter(|| black_box(rope.byte_to_line(rng.below(rope.len_bytes()))))
    });
    g.finish();

    let mut g = c.benchmark_group("scan_100MiB");
    g.sample_size(10);
    g.throughput(Throughput::Bytes(buf.len() as u64));
    g.bench_function("piece_tree_chunks_count_lf", |b| {
        b.iter(|| {
            let chunks = buf.chunks(0..buf.len()).unwrap();
            black_box(
                chunks
                    .map(|c| memchr::memchr_iter(b'\n', c).count())
                    .sum::<usize>(),
            )
        })
    });
    g.finish();
}

criterion_group!(benches, build, edits, lookups);
criterion_main!(benches);
