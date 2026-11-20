//! Measures memory retained by snapshots (as undo history would keep them).
//!
//! Builds a 100 MiB document, then performs N random single-byte inserts while
//! keeping every intermediate version alive, and reports resident memory
//! (Linux `/proc/self/status`; prints 0 elsewhere).
//!
//! `cargo run --release -p slashpad-text --example snapshot_memory`

use slashpad_text::TextBuffer;

fn rss_kib(field: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with(field))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn main() {
    let mut text = Vec::with_capacity(100 << 20);
    let mut i = 0u64;
    while text.len() < 100 << 20 {
        text.extend_from_slice(format!("2026-11-20 INFO request {i} ünïcödé\r\n").as_bytes());
        i += 1;
    }
    let len = text.len();
    let base_before = rss_kib("VmRSS:");
    let mut buf = TextBuffer::from_bytes(text); // Vec -> Arc<[u8]> copies once, then the Vec is freed
    let base = rss_kib("VmRSS:");
    println!(
        "document {} MiB, {} lines, {:?}; RSS {} MiB (before build {} MiB)",
        len >> 20,
        buf.line_count(),
        buf.stats(),
        base >> 10,
        base_before >> 10
    );

    let mut seed = 0x9e3779b97f4a7c15u64;
    let mut history = Vec::new();
    for edits in [1_000usize, 10_000, 100_000] {
        while history.len() < edits {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            history.push(buf.clone());
            let at = (seed % buf.len() as u64) as usize;
            if buf.insert(at, b"x").is_err() {
                eprintln!("insert failed");
                return;
            }
        }
        let rss = rss_kib("VmRSS:");
        println!(
            "{edits:>7} retained versions: RSS +{} MiB over base => {:.0} bytes/version; {:?}",
            (rss - base) >> 10,
            ((rss - base) * 1024) as f64 / edits as f64,
            buf.stats()
        );
    }
}
