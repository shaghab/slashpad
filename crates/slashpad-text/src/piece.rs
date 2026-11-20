//! A piece is an immutable view of a byte range inside shared storage.

use std::ops::Range;
use std::sync::Arc;

use crate::summary::Summary;

/// Maximum length of a single piece.
///
/// Bounding pieces keeps the cost of splitting one (which rescans its bytes to
/// recompute the summary) small and predictable. Tests use a tiny value so
/// that even short documents exercise deep trees.
#[cfg(not(test))]
pub(crate) const MAX_PIECE_LEN: usize = 16 * 1024;
#[cfg(test)]
pub(crate) const MAX_PIECE_LEN: usize = 7;

#[derive(Clone, Debug)]
pub(crate) struct Piece {
    data: Arc<[u8]>,
    start: usize,
    summary: Summary,
}

impl Piece {
    /// Creates a piece over `data[range]`. `range` must be in bounds.
    fn new(data: Arc<[u8]>, range: Range<usize>) -> Self {
        let summary = Summary::of(&data[range.clone()]);
        Self {
            data,
            start: range.start,
            summary,
        }
    }

    /// Splits shared storage into pieces of at most [`MAX_PIECE_LEN`] bytes.
    pub fn split_storage(data: Arc<[u8]>) -> Vec<Piece> {
        let len = data.len();
        (0..len)
            .step_by(MAX_PIECE_LEN)
            .map(|start| Piece::new(Arc::clone(&data), start..(start + MAX_PIECE_LEN).min(len)))
            .collect()
    }

    /// Copies `bytes` into new storage and splits it into pieces.
    pub fn from_bytes(bytes: &[u8]) -> Vec<Piece> {
        Self::split_storage(Arc::from(bytes))
    }

    /// A new piece holding the bytes of `a` followed by the bytes of `b`.
    pub fn concat(a: &Piece, b: &Piece) -> Piece {
        let mut joined = Vec::with_capacity(a.len() + b.len());
        joined.extend_from_slice(a.bytes());
        joined.extend_from_slice(b.bytes());
        let len = joined.len();
        Piece::new(Arc::from(joined), 0..len)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data[self.start..self.start + self.summary.bytes]
    }

    pub fn len(&self) -> usize {
        self.summary.bytes
    }

    pub fn summary(&self) -> Summary {
        self.summary
    }

    /// A piece over a sub-range of this piece (relative offsets), sharing storage.
    pub fn slice(&self, range: Range<usize>) -> Piece {
        Piece::new(
            Arc::clone(&self.data),
            self.start + range.start..self.start + range.end,
        )
    }

    /// Splits at a relative offset `0 < at < len`.
    pub fn split_at(&self, at: usize) -> (Piece, Piece) {
        (self.slice(0..at), self.slice(at..self.len()))
    }
}
