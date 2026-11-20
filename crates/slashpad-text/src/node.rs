//! Balanced B-tree of pieces with copy-on-write nodes.
//!
//! Nodes are shared through `Arc`. Mutating operations call `Arc::make_mut`,
//! which mutates in place when a node is uniquely owned and clones it first
//! when it is shared with a snapshot. An edit therefore copies only the
//! O(log n) nodes on the path it touches, and every older root stays a valid,
//! unchanging snapshot.
//!
//! Invariants (checked by [`validate`]):
//! - all leaves are at the same depth;
//! - every node has at most [`MAX_ENTRIES`] entries;
//! - internal nodes have at least one child; only the root leaf may be empty;
//! - pieces are non-empty and at most `MAX_PIECE_LEN` bytes;
//! - each node's cached summary equals the combination of its entries.

use std::ops::Range;
use std::sync::Arc;

use crate::piece::{MAX_PIECE_LEN, Piece};
use crate::summary::{Summary, find_kth_break};

#[cfg(not(test))]
pub(crate) const MAX_ENTRIES: usize = 16;
#[cfg(test)]
pub(crate) const MAX_ENTRIES: usize = 4;

/// Nodes with fewer entries are merged with a neighbour after deletions.
const MIN_ENTRIES: usize = MAX_ENTRIES / 2;

/// Consecutive small inserts (typing) are merged into the preceding piece
/// while it stays below this size, so typing does not create a piece per key.
#[cfg(not(test))]
const MERGE_LIMIT: usize = 1024;
#[cfg(test)]
const MERGE_LIMIT: usize = 5;

const _: () = assert!(MERGE_LIMIT <= MAX_PIECE_LEN);
const _: () = assert!(MIN_ENTRIES >= 1);

#[derive(Clone, Debug)]
pub(crate) struct Node {
    summary: Summary,
    kind: Kind,
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Leaf(Vec<Piece>),
    Internal(Vec<Arc<Node>>),
}

/// Something stored in a node: a piece (in leaves) or a child (in internal nodes).
trait Entry {
    fn entry_summary(&self) -> Summary;
}

impl Entry for Piece {
    fn entry_summary(&self) -> Summary {
        self.summary()
    }
}

impl Entry for Arc<Node> {
    fn entry_summary(&self) -> Summary {
        self.summary
    }
}

fn sum<E: Entry>(entries: &[E]) -> Summary {
    entries
        .iter()
        .fold(Summary::default(), |acc, e| acc.combine(e.entry_summary()))
}

/// Splits `entries` into the fewest groups of at most `MAX_ENTRIES`, with
/// sizes as equal as possible. Always returns at least one (possibly empty) group.
fn distribute<E>(entries: Vec<E>) -> Vec<Vec<E>> {
    let n = entries.len();
    let groups = n.div_ceil(MAX_ENTRIES).max(1);
    let (base, extra) = (n / groups, n % groups);
    let mut iter = entries.into_iter();
    (0..groups)
        .map(|g| iter.by_ref().take(base + usize::from(g < extra)).collect())
        .collect()
}

/// For entries of the given lengths, finds the entry that contains document
/// offset `at` for insertion: the first entry whose end is at or after `at`
/// (so inserts at a boundary go to the end of the left entry). Returns
/// `(index, offset within entry)`; for no entries returns `(0, 0)`.
fn locate_for_insert(lens: impl Iterator<Item = usize>, at: usize) -> (usize, usize) {
    let mut start = 0;
    let mut last = None;
    for (i, len) in lens.enumerate() {
        if at <= start + len {
            return (i, at - start);
        }
        start += len;
        last = Some((i, len));
    }
    // Only reachable if `at` exceeds the total length; clamp to the end.
    last.map_or((0, 0), |(i, len)| (i, len))
}

impl Node {
    fn leaf(pieces: Vec<Piece>) -> Self {
        Self {
            summary: sum(&pieces),
            kind: Kind::Leaf(pieces),
        }
    }

    fn internal(children: Vec<Arc<Node>>) -> Self {
        Self {
            summary: sum(&children),
            kind: Kind::Internal(children),
        }
    }

    pub fn empty() -> Arc<Node> {
        Arc::new(Node::leaf(Vec::new()))
    }

    pub fn summary(&self) -> Summary {
        self.summary
    }

    pub fn len(&self) -> usize {
        self.summary.bytes
    }

    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    fn entry_count(&self) -> usize {
        match &self.kind {
            Kind::Leaf(p) => p.len(),
            Kind::Internal(c) => c.len(),
        }
    }

    fn recompute(&mut self) {
        self.summary = match &self.kind {
            Kind::Leaf(p) => sum(p),
            Kind::Internal(c) => sum(c),
        };
    }

    /// Recomputes the summary and, if the node has too many entries, keeps the
    /// first group and returns the rest as new sibling nodes.
    fn split_overflow(&mut self) -> Vec<Arc<Node>> {
        let siblings = match &mut self.kind {
            Kind::Leaf(pieces) if pieces.len() > MAX_ENTRIES => {
                let mut groups = distribute(std::mem::take(pieces)).into_iter();
                *pieces = groups.next().unwrap_or_default();
                groups.map(|g| Arc::new(Node::leaf(g))).collect()
            }
            Kind::Internal(children) if children.len() > MAX_ENTRIES => {
                let mut groups = distribute(std::mem::take(children)).into_iter();
                *children = groups.next().unwrap_or_default();
                groups.map(|g| Arc::new(Node::internal(g))).collect()
            }
            _ => Vec::new(),
        };
        self.recompute();
        siblings
    }

    /// Builds a balanced tree from pieces in document order.
    pub fn build(pieces: Vec<Piece>) -> Arc<Node> {
        let leaves = distribute(pieces)
            .into_iter()
            .map(|g| Arc::new(Node::leaf(g)))
            .collect();
        Self::stack_levels(leaves)
    }

    /// Adds internal levels above `level` until a single root remains.
    fn stack_levels(mut level: Vec<Arc<Node>>) -> Arc<Node> {
        while level.len() > 1 {
            level = distribute(level)
                .into_iter()
                .map(|g| Arc::new(Node::internal(g)))
                .collect();
        }
        level.pop().unwrap_or_else(Node::empty)
    }

    /// Combines a root with the overflow siblings returned by an edit.
    pub fn grow(root: Arc<Node>, siblings: Vec<Arc<Node>>) -> Arc<Node> {
        if siblings.is_empty() {
            return root;
        }
        let mut level = Vec::with_capacity(siblings.len() + 1);
        level.push(root);
        level.extend(siblings);
        Self::stack_levels(level)
    }

    /// Removes internal roots that have a single child (after deletions).
    pub fn shrink(mut root: Arc<Node>) -> Arc<Node> {
        loop {
            let only_child = match &root.kind {
                Kind::Internal(c) if c.len() == 1 => Arc::clone(&c[0]),
                Kind::Internal(c) if c.is_empty() => return Node::empty(),
                _ => return root,
            };
            root = only_child;
        }
    }
}

/// Inserts `new` pieces at offset `at` relative to `node`. Returns overflow
/// siblings that belong immediately after `node`, at the same height.
pub(crate) fn insert(node: &mut Arc<Node>, at: usize, new: Vec<Piece>) -> Vec<Arc<Node>> {
    let n = Arc::make_mut(node);
    match &mut n.kind {
        Kind::Leaf(pieces) => {
            let (idx, within) = locate_for_insert(pieces.iter().map(Piece::len), at);
            let mut insert_at = idx;
            if let Some(piece) = pieces.get(idx) {
                if within == piece.len() {
                    insert_at = idx + 1;
                } else if within > 0 {
                    let (left, right) = piece.split_at(within);
                    pieces[idx] = left;
                    pieces.insert(idx + 1, right);
                    insert_at = idx + 1;
                }
            }
            let merge_into_prev = match (new.as_slice(), insert_at.checked_sub(1)) {
                ([single], Some(prev)) => pieces[prev].len() + single.len() <= MERGE_LIMIT,
                _ => false,
            };
            if merge_into_prev {
                let prev = insert_at - 1;
                pieces[prev] = Piece::concat(&pieces[prev], &new[0]);
            } else {
                pieces.splice(insert_at..insert_at, new);
            }
        }
        Kind::Internal(children) => {
            let (idx, within) = locate_for_insert(children.iter().map(|c| c.len()), at);
            match children.get_mut(idx) {
                Some(child) => {
                    let siblings = insert(child, within, new);
                    children.splice(idx + 1..idx + 1, siblings);
                }
                // Unreachable for a valid tree (internal nodes are non-empty).
                None => children.push(Node::build(new)),
            }
        }
    }
    n.split_overflow()
}

/// Removes `range` (relative to `node`). Returns overflow siblings, which can
/// appear when a deletion strictly inside one piece splits it in two.
pub(crate) fn delete(node: &mut Arc<Node>, range: Range<usize>) -> Vec<Arc<Node>> {
    let n = Arc::make_mut(node);
    match &mut n.kind {
        Kind::Leaf(pieces) => {
            let old = std::mem::take(pieces);
            let mut start = 0;
            for piece in old {
                let end = start + piece.len();
                if end <= range.start || start >= range.end {
                    pieces.push(piece);
                } else {
                    if range.start > start {
                        pieces.push(piece.slice(0..range.start - start));
                    }
                    if range.end < end {
                        pieces.push(piece.slice(range.end - start..piece.len()));
                    }
                }
                start = end;
            }
        }
        Kind::Internal(children) => {
            let old = std::mem::take(children);
            let mut start = 0;
            for mut child in old {
                let end = start + child.len();
                if end <= range.start || start >= range.end {
                    children.push(child);
                } else {
                    let local = range.start.saturating_sub(start)..range.end.min(end) - start;
                    let siblings = delete(&mut child, local);
                    if child.len() > 0 {
                        children.push(child);
                    }
                    children.extend(siblings.into_iter().filter(|s| s.len() > 0));
                }
                start = end;
            }
            rebalance(children);
        }
    }
    n.split_overflow()
}

/// Merges underfull children with a neighbour. All children have the same height.
fn rebalance(children: &mut Vec<Arc<Node>>) {
    let mut i = 0;
    while i < children.len() {
        if children.len() > 1 && children[i].entry_count() < MIN_ENTRIES {
            let lo = if i + 1 < children.len() { i } else { i - 1 };
            let right = children.remove(lo + 1);
            let left = children.remove(lo);
            let merged = merge(left, right);
            let count = merged.len();
            children.splice(lo..lo, merged);
            // A single merged node may still be underfull: re-check it.
            i = if count == 1 { lo } else { lo + count };
        } else {
            i += 1;
        }
    }
}

/// Concatenates the entries of two same-height nodes and redistributes them.
fn merge(a: Arc<Node>, b: Arc<Node>) -> Vec<Arc<Node>> {
    let (a, b) = (Arc::unwrap_or_clone(a), Arc::unwrap_or_clone(b));
    match (a.kind, b.kind) {
        (Kind::Leaf(mut x), Kind::Leaf(y)) => {
            x.extend(y);
            distribute(x)
                .into_iter()
                .map(|g| Arc::new(Node::leaf(g)))
                .collect()
        }
        (Kind::Internal(mut x), Kind::Internal(y)) => {
            x.extend(y);
            distribute(x)
                .into_iter()
                .map(|g| Arc::new(Node::internal(g)))
                .collect()
        }
        // Heights differ: an invariant violation. Keep both nodes unchanged.
        (x, y) => {
            debug_assert!(false, "merge of nodes with different heights");
            vec![Arc::new(Node::from_kind(x)), Arc::new(Node::from_kind(y))]
        }
    }
}

impl Node {
    fn from_kind(kind: Kind) -> Self {
        match kind {
            Kind::Leaf(p) => Node::leaf(p),
            Kind::Internal(c) => Node::internal(c),
        }
    }
}

// ---------------------------------------------------------------------------
// Queries. All are iterative descents: O(height * MAX_ENTRIES) plus at most
// one scan of a single piece.
// ---------------------------------------------------------------------------

/// The byte at `offset`, if in bounds.
pub(crate) fn byte_at(root: &Node, mut offset: usize) -> Option<u8> {
    let mut node = root;
    loop {
        match &node.kind {
            Kind::Internal(children) => {
                let mut next = None;
                for child in children {
                    if offset < child.len() {
                        next = Some(child);
                        break;
                    }
                    offset -= child.len();
                }
                node = next?;
            }
            Kind::Leaf(pieces) => {
                for piece in pieces {
                    if offset < piece.len() {
                        return piece.bytes().get(offset).copied();
                    }
                    offset -= piece.len();
                }
                return None;
            }
        }
    }
}

/// Summary of the bytes in `0..offset` (clamped to the document length).
pub(crate) fn prefix_summary(root: &Node, mut offset: usize) -> Summary {
    let mut acc = Summary::default();
    let mut node = root;
    loop {
        match &node.kind {
            Kind::Internal(children) => {
                let mut next = None;
                for child in children {
                    if offset < child.len() {
                        next = Some(child);
                        break;
                    }
                    offset -= child.len();
                    acc = acc.combine(child.summary);
                }
                match next {
                    Some(child) => node = child,
                    None => return acc,
                }
            }
            Kind::Leaf(pieces) => {
                for piece in pieces {
                    if offset < piece.len() {
                        return acc.combine(Summary::of(&piece.bytes()[..offset]));
                    }
                    offset -= piece.len();
                    acc = acc.combine(piece.summary());
                }
                return acc;
            }
        }
    }
}

/// Document offset of the byte that forms the `k`-th (1-based) line break.
/// For a CRLF this is the offset of the CR.
pub(crate) fn find_break(root: &Node, mut k: usize) -> Option<usize> {
    if k == 0 || k > root.summary.breaks {
        return None;
    }
    let mut base = 0;
    let mut node = root;
    loop {
        match &node.kind {
            Kind::Internal(children) => {
                let (idx, local_k) = select_break(children.iter().map(|c| c.summary), k)?;
                for child in &children[..idx] {
                    base += child.len();
                }
                k = local_k;
                node = &children[idx];
            }
            Kind::Leaf(pieces) => {
                let (idx, local_k) = select_break(pieces.iter().map(Piece::summary), k)?;
                for piece in &pieces[..idx] {
                    base += piece.len();
                }
                return find_kth_break(pieces[idx].bytes(), local_k).map(|p| base + p);
            }
        }
    }
}

/// Given entry summaries, returns the index of the entry containing the
/// `k`-th break and the break's index counted locally within that entry.
///
/// When the previous entries end in CR and this entry starts with LF, that
/// LF is counted locally but is not a real break, hence the `+ joined`.
fn select_break(summaries: impl Iterator<Item = Summary>, k: usize) -> Option<(usize, usize)> {
    let mut acc = Summary::default();
    for (i, s) in summaries.enumerate() {
        let combined = acc.combine(s);
        if combined.breaks >= k {
            let joined = usize::from(acc.last_cr && s.first_lf);
            return Some((i, k - acc.breaks + joined));
        }
        acc = combined;
    }
    None
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Shape statistics, for tests and benchmarks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TreeStats {
    pub height: usize,
    pub internal_nodes: usize,
    pub leaves: usize,
    pub pieces: usize,
}

pub(crate) fn stats(root: &Node) -> TreeStats {
    fn walk(node: &Node, depth: usize, s: &mut TreeStats) {
        s.height = s.height.max(depth);
        match &node.kind {
            Kind::Leaf(p) => {
                s.leaves += 1;
                s.pieces += p.len();
            }
            Kind::Internal(c) => {
                s.internal_nodes += 1;
                for child in c {
                    walk(child, depth + 1, s);
                }
            }
        }
    }
    let mut s = TreeStats::default();
    walk(root, 1, &mut s);
    s
}

/// Checks all structural invariants, returning a description of the first violation.
pub(crate) fn validate(root: &Node) -> Result<(), String> {
    fn walk(node: &Node, is_root: bool) -> Result<usize, String> {
        if node.entry_count() > MAX_ENTRIES {
            return Err(format!("node has {} entries", node.entry_count()));
        }
        let height = match &node.kind {
            Kind::Leaf(pieces) => {
                if pieces.is_empty() && !is_root {
                    return Err("empty non-root leaf".into());
                }
                for p in pieces {
                    if p.len() == 0 || p.len() > MAX_PIECE_LEN {
                        return Err(format!("piece length {}", p.len()));
                    }
                    if p.summary() != Summary::of(p.bytes()) {
                        return Err("stale piece summary".into());
                    }
                }
                1
            }
            Kind::Internal(children) => {
                let mut heights = children.iter().map(|c| walk(c, false));
                let first = heights.next().ok_or("internal node without children")??;
                for h in heights {
                    if h? != first {
                        return Err("leaves at different depths".into());
                    }
                }
                first + 1
            }
        };
        let expected = match &node.kind {
            Kind::Leaf(p) => sum(p),
            Kind::Internal(c) => sum(c),
        };
        if node.summary != expected {
            return Err("stale node summary".into());
        }
        Ok(height)
    }
    walk(root, true).map(|_| ())
}
