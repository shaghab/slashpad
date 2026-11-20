//! Persistent piece-tree text buffer for SlashPad.
//!
//! A [`TextBuffer`] stores a document as a balanced tree of *pieces*, each an
//! immutable view into shared byte storage. Key properties:
//!
//! - **Bytes, not strings.** Content is arbitrary bytes, normally UTF-8. Invalid
//!   sequences are stored and returned unchanged, so untouched regions of a
//!   file round-trip exactly (ADR-0004).
//! - **Cheap snapshots.** `clone()` is O(1). Edits copy only the O(log n) nodes
//!   they touch, so a clone never observes later edits. Undo history, split
//!   views and background workers all rely on this (ADR-0002).
//! - **Line breaks** are LF, CR and CRLF; a CRLF counts as one break even when
//!   it spans two pieces.
//!
//! All positions are byte offsets. Grapheme clusters and visual positions are
//! the editor's concern, computed for visible text only.
//!
//! Status: Milestone 0 prototype. Sources are in-memory only; file-backed
//! sources with background line indexing arrive in Milestone 4.

#![forbid(unsafe_code)]

mod chunks;
mod node;
mod piece;
mod summary;

#[cfg(test)]
mod tests;

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

pub use chunks::Chunks;
pub use node::TreeStats;

use node::Node;
use piece::Piece;

// Offsets are `usize`; SlashPad targets 64-bit Windows only, where this covers
// files far larger than any disk.
const _: () = assert!(usize::BITS >= 64);

/// A text document's contents as a persistent piece tree.
#[derive(Clone, Debug)]
pub struct TextBuffer {
    root: Arc<Node>,
}

/// An edit referred to bytes outside the buffer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// `offset` is greater than the buffer length `len`.
    OutOfBounds { offset: usize, len: usize },
    /// The range's start is after its end.
    InvalidRange { start: usize, end: usize },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::OutOfBounds { offset, len } => {
                write!(
                    f,
                    "offset {offset} is beyond the end of the buffer ({len} bytes)"
                )
            }
            EditError::InvalidRange { start, end } => {
                write!(f, "range start {start} is after its end {end}")
            }
        }
    }
}

impl std::error::Error for EditError {}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextBuffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self {
            root: Node::empty(),
        }
    }

    /// A buffer whose content is `bytes`. Takes ownership of the storage
    /// without copying when given an `Arc<[u8]>`.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self {
            root: Node::build(Piece::split_storage(bytes.into())),
        }
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.root.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of lines: line breaks + 1. An empty buffer has one (empty) line.
    pub fn line_count(&self) -> usize {
        self.root.summary().breaks + 1
    }

    /// Inserts `bytes` at byte offset `at` (`0..=len`).
    pub fn insert(&mut self, at: usize, bytes: &[u8]) -> Result<(), EditError> {
        let len = self.len();
        if at > len {
            return Err(EditError::OutOfBounds { offset: at, len });
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let siblings = node::insert(&mut self.root, at, Piece::from_bytes(bytes));
        self.root = Node::grow(Arc::clone(&self.root), siblings);
        Ok(())
    }

    /// Deletes the bytes in `range`.
    pub fn delete(&mut self, range: Range<usize>) -> Result<(), EditError> {
        self.check_range(&range)?;
        if range.is_empty() {
            return Ok(());
        }
        let siblings = node::delete(&mut self.root, range);
        self.root = Node::shrink(Node::grow(Arc::clone(&self.root), siblings));
        Ok(())
    }

    /// Replaces the bytes in `range` with `bytes`.
    pub fn replace(&mut self, range: Range<usize>, bytes: &[u8]) -> Result<(), EditError> {
        self.check_range(&range)?;
        let start = range.start;
        self.delete(range)?;
        self.insert(start, bytes)
    }

    /// The byte at `offset`, or `None` if out of bounds.
    pub fn byte_at(&self, offset: usize) -> Option<u8> {
        node::byte_at(&self.root, offset)
    }

    /// Byte offset where line `line` (0-based) starts, or `None` if there is no such line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        if line == 0 {
            return Some(0);
        }
        let pos = node::find_break(&self.root, line)?;
        let is_crlf = self.byte_at(pos) == Some(b'\r') && self.byte_at(pos + 1) == Some(b'\n');
        Some(pos + if is_crlf { 2 } else { 1 })
    }

    /// The 0-based line containing byte `offset` (clamped to the buffer
    /// length). An offset between the CR and LF of a CRLF belongs to the line
    /// that the CRLF ends.
    pub fn line_of_offset(&self, offset: usize) -> usize {
        let offset = offset.min(self.len());
        let prefix = node::prefix_summary(&self.root, offset);
        let inside_crlf = prefix.last_cr && self.byte_at(offset) == Some(b'\n');
        prefix.breaks - usize::from(inside_crlf)
    }

    /// Iterates the bytes of `range` as borrowed chunks (no copying).
    pub fn chunks(&self, range: Range<usize>) -> Result<Chunks<'_>, EditError> {
        self.check_range(&range)?;
        Ok(Chunks::new(&self.root, range))
    }

    /// Copies the bytes of `range` into a new vector.
    pub fn slice_to_vec(&self, range: Range<usize>) -> Result<Vec<u8>, EditError> {
        // `chunks` validates the range, so the capacity below is bounded by `len()`.
        let chunks = self.chunks(range.clone())?;
        let mut out = Vec::with_capacity(range.len());
        for chunk in chunks {
            out.extend_from_slice(chunk);
        }
        Ok(out)
    }

    /// Copies the whole content into a new vector.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len());
        for chunk in Chunks::new(&self.root, 0..self.len()) {
            out.extend_from_slice(chunk);
        }
        out
    }

    /// True if both buffers share the same root, i.e. are the same version.
    pub fn ptr_eq(&self, other: &TextBuffer) -> bool {
        Arc::ptr_eq(&self.root, &other.root)
    }

    /// Tree shape statistics.
    pub fn stats(&self) -> TreeStats {
        node::stats(&self.root)
    }

    /// Checks internal invariants. Intended for tests and debugging.
    #[doc(hidden)]
    pub fn validate(&self) -> Result<(), String> {
        node::validate(&self.root)
    }

    fn check_range(&self, range: &Range<usize>) -> Result<(), EditError> {
        if range.start > range.end {
            return Err(EditError::InvalidRange {
                start: range.start,
                end: range.end,
            });
        }
        let len = self.len();
        if range.end > len {
            return Err(EditError::OutOfBounds {
                offset: range.end,
                len,
            });
        }
        Ok(())
    }
}
