//! Per-node text summaries that combine associatively (a monoid).
//!
//! Line breaks are CR, LF and CRLF. A CRLF may be split across two pieces or
//! two subtrees, so a summary also remembers whether its text starts with LF
//! and ends with CR; `combine` subtracts one break when the two halves join
//! into a CRLF.

/// Aggregate information about a run of bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    /// Number of bytes.
    pub bytes: usize,
    /// Number of line breaks, counting each CRLF once. A leading LF counts as
    /// a break here; `combine` corrects it if the preceding text ends in CR.
    pub breaks: usize,
    /// The first byte is `\n`.
    pub first_lf: bool,
    /// The last byte is `\r`.
    pub last_cr: bool,
}

impl Summary {
    /// Summarises a byte slice in a single SIMD-accelerated pass.
    pub fn of(bytes: &[u8]) -> Self {
        let mut breaks = 0;
        for pos in memchr::memchr2_iter(b'\r', b'\n', bytes) {
            if !is_lf_of_crlf(bytes, pos) {
                breaks += 1;
            }
        }
        Self {
            bytes: bytes.len(),
            breaks,
            first_lf: bytes.first() == Some(&b'\n'),
            last_cr: bytes.last() == Some(&b'\r'),
        }
    }

    /// Summary of `self` followed immediately by `rhs`.
    pub fn combine(self, rhs: Self) -> Self {
        if self.bytes == 0 {
            return rhs;
        }
        if rhs.bytes == 0 {
            return self;
        }
        let joined = usize::from(self.last_cr && rhs.first_lf);
        Self {
            bytes: self.bytes + rhs.bytes,
            breaks: self.breaks + rhs.breaks - joined,
            first_lf: self.first_lf,
            last_cr: rhs.last_cr,
        }
    }
}

/// True if `bytes[pos]` is the LF of a CRLF pair that lies entirely in `bytes`.
pub(crate) fn is_lf_of_crlf(bytes: &[u8], pos: usize) -> bool {
    bytes[pos] == b'\n' && pos > 0 && bytes[pos - 1] == b'\r'
}

/// Position of the `k`-th (1-based) line-break byte in `bytes`, using the same
/// counting rule as [`Summary::of`]. For a CRLF the position of the CR is returned.
pub(crate) fn find_kth_break(bytes: &[u8], k: usize) -> Option<usize> {
    let mut count = 0;
    for pos in memchr::memchr2_iter(b'\r', b'\n', bytes) {
        if is_lf_of_crlf(bytes, pos) {
            continue;
        }
        count += 1;
        if count == k {
            return Some(pos);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_break_style_once() {
        assert_eq!(Summary::of(b"a\r\nb\nc\rd").breaks, 3);
        assert_eq!(Summary::of(b"\r\r\n\n").breaks, 3);
        assert_eq!(Summary::of(b"").breaks, 0);
    }

    #[test]
    fn combine_joins_split_crlf() {
        let joined = Summary::of(b"a\r").combine(Summary::of(b"\nb"));
        assert_eq!(joined, Summary::of(b"a\r\nb"));
        assert_eq!(joined.breaks, 1);
    }

    #[test]
    fn combine_is_associative_on_examples() {
        let parts: [&[u8]; 3] = [b"x\r", b"\n\r", b"\ny"];
        let s: Vec<Summary> = parts.iter().map(|p| Summary::of(p)).collect();
        assert_eq!(
            s[0].combine(s[1]).combine(s[2]),
            s[0].combine(s[1].combine(s[2]))
        );
        assert_eq!(s[0].combine(s[1]).combine(s[2]), Summary::of(b"x\r\n\r\ny"));
    }

    #[test]
    fn kth_break_points_at_cr_of_crlf() {
        let text = b"a\r\nb\nc\r";
        assert_eq!(find_kth_break(text, 1), Some(1));
        assert_eq!(find_kth_break(text, 2), Some(4));
        assert_eq!(find_kth_break(text, 3), Some(6));
        assert_eq!(find_kth_break(text, 4), None);
    }
}
