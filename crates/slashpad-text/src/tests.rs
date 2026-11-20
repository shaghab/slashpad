//! Unit and property tests. Under `cfg(test)` the tree uses tiny constants
//! (pieces of 7 bytes, 4 entries per node) so short inputs build deep trees.

use proptest::prelude::*;

use crate::{EditError, TextBuffer};

/// Reference line starts for `text` using the same rules as the buffer.
fn oracle_line_starts(text: &[u8]) -> Vec<usize> {
    let mut starts = vec![0];
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'\r' => {
                if text.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                starts.push(i + 1);
            }
            b'\n' => starts.push(i + 1),
            _ => {}
        }
        i += 1;
    }
    starts
}

/// Compares every query against the oracle. Exhaustive, so only for small texts.
fn assert_matches_oracle(buf: &TextBuffer, oracle: &[u8]) {
    buf.validate().unwrap();
    assert_eq!(buf.len(), oracle.len());
    assert_eq!(buf.to_vec(), oracle);

    let starts = oracle_line_starts(oracle);
    assert_eq!(buf.line_count(), starts.len());
    for (line, &start) in starts.iter().enumerate() {
        assert_eq!(buf.line_start(line), Some(start), "line_start({line})");
    }
    assert_eq!(buf.line_start(starts.len()), None);

    for offset in 0..=oracle.len() {
        let expected = starts.iter().filter(|&&s| s <= offset).count() - 1;
        assert_eq!(
            buf.line_of_offset(offset),
            expected,
            "line_of_offset({offset})"
        );
        assert_eq!(buf.byte_at(offset), oracle.get(offset).copied());
    }
}

#[derive(Clone, Debug)]
enum Op {
    /// Insert at `pos % (len + 1)`.
    Insert(usize, Vec<u8>),
    /// Delete `len` bytes starting at `pos % (len + 1)`, clamped to the end.
    Delete(usize, usize),
}

/// Bytes biased towards line breaks and multi-byte UTF-8 fragments.
fn text_bytes(max: usize) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop_oneof![
            3 => Just(b'a'),
            2 => Just(b'\n'),
            2 => Just(b'\r'),
            1 => Just(0xC3u8),
            1 => Just(0xA9u8),
        ],
        0..max,
    )
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (any::<usize>(), text_bytes(12)).prop_map(|(p, b)| Op::Insert(p, b)),
        1 => (any::<usize>(), text_bytes(60)).prop_map(|(p, b)| Op::Insert(p, b)),
        3 => (any::<usize>(), 0usize..20).prop_map(|(p, n)| Op::Delete(p, n)),
    ]
}

fn apply(buf: &mut TextBuffer, oracle: &mut Vec<u8>, op: &Op) {
    match op {
        Op::Insert(pos, bytes) => {
            let at = pos % (oracle.len() + 1);
            buf.insert(at, bytes).unwrap();
            oracle.splice(at..at, bytes.iter().copied());
        }
        Op::Delete(pos, n) => {
            let start = pos % (oracle.len() + 1);
            let end = (start + n).min(oracle.len());
            buf.delete(start..end).unwrap();
            oracle.drain(start..end);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn random_edits_match_oracle(initial in text_bytes(200), ops in prop::collection::vec(op(), 0..60)) {
        let mut buf = TextBuffer::from_bytes(initial.clone());
        let mut oracle = initial;
        assert_matches_oracle(&buf, &oracle);
        for op in &ops {
            apply(&mut buf, &mut oracle, op);
            buf.validate().unwrap();
            prop_assert_eq!(buf.len(), oracle.len());
        }
        assert_matches_oracle(&buf, &oracle);
    }

    #[test]
    fn snapshots_are_unaffected_by_later_edits(
        initial in text_bytes(150),
        ops in prop::collection::vec(op(), 1..40),
    ) {
        let mut buf = TextBuffer::from_bytes(initial.clone());
        let mut oracle = initial;
        let mut snapshots = Vec::new();
        for op in &ops {
            snapshots.push((buf.clone(), oracle.clone()));
            apply(&mut buf, &mut oracle, op);
        }
        for (snapshot, expected) in &snapshots {
            snapshot.validate().unwrap();
            prop_assert_eq!(&snapshot.to_vec(), expected);
        }
        assert_matches_oracle(&buf, &oracle);
    }

    #[test]
    fn chunks_cover_any_range(
        initial in text_bytes(200),
        ops in prop::collection::vec(op(), 0..20),
        a in any::<usize>(),
        b in any::<usize>(),
    ) {
        let mut buf = TextBuffer::from_bytes(initial.clone());
        let mut oracle = initial;
        for op in &ops {
            apply(&mut buf, &mut oracle, op);
        }
        let (x, y) = (a % (oracle.len() + 1), b % (oracle.len() + 1));
        let range = x.min(y)..x.max(y);
        let chunks: Vec<&[u8]> = buf.chunks(range.clone()).unwrap().collect();
        prop_assert!(chunks.iter().all(|c| !c.is_empty()));
        prop_assert_eq!(chunks.concat(), &oracle[range]);
    }
}

#[test]
fn empty_buffer() {
    let buf = TextBuffer::new();
    assert!(buf.is_empty());
    assert_eq!(buf.line_count(), 1);
    assert_eq!(buf.line_start(0), Some(0));
    assert_eq!(buf.line_start(1), None);
    assert_eq!(buf.line_of_offset(0), 0);
    assert_eq!(buf.chunks(0..0).unwrap().count(), 0);
    buf.validate().unwrap();
}

#[test]
fn crlf_split_by_insert_and_rejoined_by_delete() {
    let mut buf = TextBuffer::from_bytes(b"ab\r\ncd".to_vec());
    assert_eq!(buf.line_count(), 2);
    buf.insert(3, b"X").unwrap(); // "ab\rX\ncd": CR and LF are now separate breaks
    assert_eq!(buf.line_count(), 3);
    buf.delete(3..4).unwrap(); // CR and LF in different pieces form a CRLF again
    assert_eq!(buf.line_count(), 2);
    assert_eq!(buf.line_start(1), Some(4));
    assert_matches_oracle(&buf, b"ab\r\ncd");
}

#[test]
fn delete_everything_then_reuse() {
    let text: Vec<u8> = (0..500u32)
        .flat_map(|i| format!("{i}\n").into_bytes())
        .collect();
    let mut buf = TextBuffer::from_bytes(text.clone());
    assert!(buf.stats().height > 2);
    buf.delete(0..text.len()).unwrap();
    assert!(buf.is_empty());
    assert_eq!(buf.stats().height, 1);
    buf.insert(0, b"hello").unwrap();
    assert_matches_oracle(&buf, b"hello");
}

#[test]
fn typing_merges_small_inserts() {
    let mut buf = TextBuffer::new();
    for (i, b) in b"abcde".iter().enumerate() {
        buf.insert(i, &[*b]).unwrap();
    }
    // MERGE_LIMIT is 5 under cfg(test): five keystrokes fit in one piece.
    assert_eq!(buf.stats().pieces, 1);
    assert_matches_oracle(&buf, b"abcde");
}

#[test]
fn errors_on_out_of_bounds() {
    let mut buf = TextBuffer::from_bytes(b"abc".to_vec());
    assert_eq!(
        buf.insert(4, b"x"),
        Err(EditError::OutOfBounds { offset: 4, len: 3 })
    );
    assert_eq!(
        buf.delete(1..5),
        Err(EditError::OutOfBounds { offset: 5, len: 3 })
    );
    #[allow(clippy::reversed_empty_ranges)]
    let reversed = 2..1;
    assert_eq!(
        buf.delete(reversed),
        Err(EditError::InvalidRange { start: 2, end: 1 })
    );
    assert_eq!(buf.to_vec(), b"abc");
}

#[test]
fn invalid_utf8_is_preserved() {
    let bytes = vec![b'a', 0xFF, 0xFE, b'\n', 0xC3];
    let mut buf = TextBuffer::from_bytes(bytes.clone());
    buf.insert(5, &[0xA9]).unwrap();
    let mut expected = bytes;
    expected.push(0xA9);
    assert_matches_oracle(&buf, &expected);
}

#[test]
fn slice_to_vec_rejects_huge_range_without_allocating() {
    // Regression test for shaghab/slashpad#2: the range must be validated
    // before the output vector reserves `range.len()` bytes.
    let buf = TextBuffer::new();
    assert_eq!(
        buf.slice_to_vec(0..usize::MAX),
        Err(EditError::OutOfBounds {
            offset: usize::MAX,
            len: 0
        })
    );
}
