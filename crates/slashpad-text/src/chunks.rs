//! Borrowing iterator over the contiguous byte runs of a range.

use std::ops::Range;
use std::slice;
use std::sync::Arc;

use crate::node::{Kind, Node};
use crate::piece::Piece;

/// Iterator over `&[u8]` chunks covering a byte range of a [`TextBuffer`],
/// in document order, without copying.
///
/// The chunks borrow from the buffer (lifetime `'a`), so the buffer cannot be
/// edited while the iterator is alive. To read while editing elsewhere, clone
/// the buffer first (an O(1) snapshot) and iterate the clone.
///
/// [`TextBuffer`]: crate::TextBuffer
#[derive(Debug)]
pub struct Chunks<'a> {
    /// Remaining children at each internal level above the current leaf.
    stack: Vec<slice::Iter<'a, Arc<Node>>>,
    pieces: slice::Iter<'a, Piece>,
    /// Document offset of the next piece yielded by `pieces`.
    pos: usize,
    start: usize,
    end: usize,
}

impl<'a> Chunks<'a> {
    pub(crate) fn new(root: &'a Node, range: Range<usize>) -> Self {
        let mut stack = Vec::new();
        let mut pos = 0;
        let mut node = root;
        let pieces = loop {
            match node.kind() {
                Kind::Leaf(pieces) => break pieces.iter(),
                Kind::Internal(children) => {
                    let mut iter = children.iter();
                    let mut chosen = None;
                    for child in iter.by_ref() {
                        if pos + child.len() > range.start {
                            chosen = Some(child);
                            break;
                        }
                        pos += child.len();
                    }
                    stack.push(iter);
                    match chosen {
                        Some(child) => node = child,
                        None => break [].iter(),
                    }
                }
            }
        };
        Self {
            stack,
            pieces,
            pos,
            start: range.start,
            end: range.end,
        }
    }

    /// Moves to the leftmost leaf after the current one. Returns false at the end.
    fn advance_leaf(&mut self) -> bool {
        loop {
            let next = match self.stack.last_mut() {
                None => return false,
                Some(iter) => iter.next(),
            };
            let Some(mut node) = next else {
                self.stack.pop();
                continue;
            };
            loop {
                match node.kind() {
                    Kind::Leaf(pieces) => {
                        self.pieces = pieces.iter();
                        return true;
                    }
                    Kind::Internal(children) => {
                        let mut iter = children.iter();
                        let first = iter.next();
                        self.stack.push(iter);
                        match first {
                            Some(child) => node = child,
                            None => break,
                        }
                    }
                }
            }
        }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        if self.start >= self.end {
            return None;
        }
        loop {
            if self.pos >= self.end {
                return None;
            }
            match self.pieces.next() {
                Some(piece) => {
                    let piece_start = self.pos;
                    self.pos += piece.len();
                    if self.pos <= self.start {
                        continue;
                    }
                    let lo = self.start.saturating_sub(piece_start);
                    let hi = piece.len().min(self.end - piece_start);
                    return Some(&piece.bytes()[lo..hi]);
                }
                None => {
                    if !self.advance_leaf() {
                        return None;
                    }
                }
            }
        }
    }
}
