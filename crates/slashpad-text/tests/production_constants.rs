//! Tests against the public API with production tree constants (64 KiB
//! pieces, 16 entries per node), complementing the small-constant unit tests.

use slashpad_text::TextBuffer;

/// Deterministic xorshift generator; avoids a `rand` dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn log_text(lines: usize) -> Vec<u8> {
    (0..lines)
        .flat_map(|i| format!("2026-11-20 INFO request {i} ünïcödé\r\n").into_bytes())
        .collect()
}

fn line_starts(text: &[u8]) -> Vec<usize> {
    let mut starts = vec![0];
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' && text.get(i + 1) == Some(&b'\n') {
            i += 1;
        }
        if text[i] == b'\r' || text[i] == b'\n' {
            starts.push(i + 1);
        }
        i += 1;
    }
    starts
}

#[test]
fn many_random_edits_on_a_multi_megabyte_document() {
    let mut oracle = log_text(60_000); // ~2.4 MB
    let mut buf = TextBuffer::from_bytes(oracle.clone());
    let mut rng = Rng(0x5eed);
    for step in 0..3_000 {
        let at = rng.below(oracle.len() + 1);
        if rng.below(3) == 0 {
            let end = (at + rng.below(5_000)).min(oracle.len());
            buf.delete(at..end).unwrap();
            oracle.drain(at..end);
        } else {
            let text = if rng.below(50) == 0 {
                log_text(2_000)
            } else {
                b"x\r\ny".to_vec()
            };
            buf.insert(at, &text).unwrap();
            oracle.splice(at..at, text);
        }
        if step % 500 == 0 {
            buf.validate().unwrap();
        }
    }
    buf.validate().unwrap();
    assert_eq!(buf.to_vec(), oracle);
    let starts = line_starts(&oracle);
    assert_eq!(buf.line_count(), starts.len());
    for _ in 0..2_000 {
        let line = rng.below(starts.len());
        assert_eq!(buf.line_start(line), Some(starts[line]));
        let offset = rng.below(oracle.len() + 1);
        let expected = starts.partition_point(|&s| s <= offset) - 1;
        assert_eq!(buf.line_of_offset(offset), expected);
    }
}

#[test]
fn large_paste_and_full_delete() {
    let text = log_text(200_000); // ~8 MB, ~120 pieces
    let mut buf = TextBuffer::new();
    buf.insert(0, &text).unwrap();
    buf.validate().unwrap();
    assert_eq!(buf.len(), text.len());
    assert_eq!(buf.line_count(), 200_001);
    let snapshot = buf.clone();
    buf.delete(0..text.len()).unwrap();
    assert!(buf.is_empty());
    assert_eq!(snapshot.to_vec(), text);
}

#[test]
fn typing_keeps_piece_count_low() {
    let mut buf = TextBuffer::from_bytes(log_text(10_000));
    let before = buf.stats().pieces;
    let start = buf.len() / 2;
    for at in start..start + 10_000 {
        buf.insert(at, b"k").unwrap();
    }
    // 10 000 keystrokes merge into pieces of up to 1 KiB: ~10 new pieces, not 10 000.
    let added = buf.stats().pieces - before;
    assert!(added <= 12, "typing added {added} pieces");
}
