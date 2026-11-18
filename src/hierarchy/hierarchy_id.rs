use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use super::ordpath::{DefaultEncoding, OrdPathBuf};

/// A hierarchical position identifier for filesystem tree nodes.
///
/// Wraps an OrdPath-encoded byte sequence. Byte comparison gives depth-first
/// tree ordering; prefix matching gives ancestor/descendant. Outside the
/// `hierarchy` module, callers must treat the contents as opaque: positional
/// operations are expressed via [`HierarchyId::get_descendant`], not by
/// touching ordinals directly.
///
/// # Ordinals, levels and carets
///
/// A position is a sequence of ordinals. Odd ordinals are levels of the
/// tree; even ordinals are "carets" (ORDPATH, O'Neil et al., SIGMOD 2004
/// §3.3) that make room between two siblings without adding a level. A
/// node's step below its parent is therefore zero or more carets followed
/// by one odd ordinal: `5`, `6.1`, `6.-1`, `6.2.3`. `3.6.1` is a child of
/// `3`, a sibling between `3.5` and `3.7`.
///
/// Because every step ends in an odd ordinal and carets are even, no
/// node's step is the beginning of a sibling's step. That is what makes
/// "shares my prefix" mean exactly "is beneath me".
///
/// API shape mirrors SQL Server's `SqlHierarchyId` (`GetRoot`, `GetDescendant`,
/// `GetAncestor`, `GetLevel`, `IsDescendantOf`).
#[derive(Clone)]
pub struct HierarchyId {
    inner: OrdPathBuf<DefaultEncoding>,
}

impl HierarchyId {
    // ── Public API (opaque, positional) ──────────────────────────────

    /// Returns the root node of the hierarchy. The root sits above all
    /// stored items; it has no bytes and no level. Use it as the parent
    /// when computing positions for top-level items.
    pub fn get_root() -> HierarchyId {
        HierarchyId::from_ordinals(&[])
    }

    /// Returns the level of this node (root = 0, top-level child = 1).
    /// Carets are not levels.
    pub fn get_level(&self) -> i32 {
        self.inner.ordinals().filter(|ord| is_odd(*ord)).count() as i32
    }

    /// Returns the ancestor `n` levels above this node, or `None` if `n`
    /// exceeds this node's level. `get_ancestor(0)` returns a clone of self;
    /// `get_ancestor(1)` returns the immediate parent.
    pub fn get_ancestor(&self, n: i32) -> Option<HierarchyId> {
        if n < 0 {
            return None;
        }
        let mut ords = self.ordinals();
        for _ in 0..n {
            // Drop this level's odd ordinal, then the carets that led to it.
            ords.pop()?;
            while ords.last().is_some_and(|ord| !is_odd(*ord)) {
                ords.pop();
            }
        }
        Some(HierarchyId::from_ordinals(&ords))
    }

    /// The exclusive upper bound of this node's subtree in byte order:
    /// every descendant sorts after the node and before this value, and
    /// nothing else does. `None` for the root, whose subtree is
    /// everything.
    ///
    /// The value is the node with its last ordinal raised by one. It is
    /// not a position any node holds (it ends in a caret).
    pub fn subtree_end(&self) -> Option<Vec<u8>> {
        let mut ords = self.ordinals();
        *ords.last_mut()? += 1;
        Some(HierarchyId::from_ordinals(&ords).to_vec())
    }

    /// Returns a new descendant of `self` whose position sorts strictly
    /// between `child1` and `child2`.
    ///
    /// - `(None, None)` — first child of `self`.
    /// - `(Some(l), None)` — sibling immediately after `l`.
    /// - `(None, Some(r))` — sibling immediately before `r`.
    /// - `(Some(l), Some(r))` — sibling strictly between `l` and `r`.
    ///
    /// When supplied, `child1` and `child2` must be children of `self`
    /// with nothing between them, and `child1 < child2` must hold. Violations are caught by `debug_assert`
    /// in debug builds; release builds trust the caller.
    pub fn get_descendant(
        &self,
        child1: Option<&HierarchyId>,
        child2: Option<&HierarchyId>,
    ) -> HierarchyId {
        if let Some(c) = child1 {
            debug_assert!(
                c.is_descendant_of(self),
                "child1 must be a descendant of self"
            );
        }
        if let Some(c) = child2 {
            debug_assert!(
                c.is_descendant_of(self),
                "child2 must be a descendant of self"
            );
        }
        if let (Some(l), Some(r)) = (child1, child2) {
            debug_assert!(l < r, "child1 must be less than child2");
        }

        // Work on each child's step below `self`: the ordinals after
        // `self`'s own.
        let mut ords = self.ordinals();
        let depth = ords.len();
        let left = child1.map(|c| c.ordinals());
        let right = child2.map(|c| c.ordinals());
        let step = step_between(
            left.as_ref().map(|o| &o[depth..]),
            right.as_ref().map(|o| &o[depth..]),
        );
        ords.extend(step);
        HierarchyId::from_ordinals(&ords)
    }

    /// True if `self` is a descendant of `other` (i.e. `other` is one of
    /// `self`'s ancestors). A node is not a descendant of itself.
    pub fn is_descendant_of(&self, other: &HierarchyId) -> bool {
        self.inner.is_descendant_of(&other.inner)
    }

    // ── Opaque BLOB round-trip (for SQLite storage) ──────────────────

    /// Construct from raw bytes (as stored in a SQLite BLOB column).
    pub fn from_bytes(bytes: &[u8]) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_bytes(bytes, DefaultEncoding),
        }
    }

    /// Borrow the raw bytes for storage in a BLOB column.
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.as_ref()
    }

    /// Copy the raw bytes into an owned `Vec<u8>`.
    pub fn to_vec(&self) -> Vec<u8> {
        self.inner.as_ref().to_vec()
    }

    // ── Internal helpers (ordinal-aware, not part of public API) ─────

    /// Construct from a sequence of ordinals.
    pub(super) fn from_ordinals(ordinals: &[i64]) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_ordinals(ordinals, DefaultEncoding),
        }
    }

    /// Append `ordinal` to `parent`'s ordinals to form a child position.
    pub(super) fn child_of(parent: &HierarchyId, ordinal: i64) -> HierarchyId {
        let mut ords = parent.ordinals();
        ords.push(ordinal);
        HierarchyId::from_ordinals(&ords)
    }

    /// Return this node's ordinal components.
    pub(super) fn ordinals(&self) -> Vec<i64> {
        self.inner.ordinals().collect()
    }

}

fn is_odd(ordinal: i64) -> bool {
    ordinal.rem_euclid(2) == 1
}

/// The smallest odd ordinal above `ordinal`.
fn next_odd(ordinal: i64) -> i64 {
    if is_odd(ordinal) {
        ordinal + 2
    } else {
        ordinal + 1
    }
}

/// The largest odd ordinal below `ordinal`.
fn previous_odd(ordinal: i64) -> i64 {
    if is_odd(ordinal) {
        ordinal - 2
    } else {
        ordinal - 1
    }
}

/// A step (carets, then one odd ordinal) that sorts strictly between two
/// adjacent siblings' steps. `None` on a side means there is no sibling
/// there.
///
/// The result never begins with a whole sibling step, so the new node is
/// never mistaken for something beneath its neighbor:
///
/// - With room for an odd ordinal between the two first ordinals, use it:
///   between `3` and `7` is `5`.
/// - With room only for an even one, open a caret: between `3` and `5` is
///   `4.1`.
/// - With no room (first ordinals `n` and `n + 1`), one of the two is a
///   caret. Go inside it, next to the sibling that is already there:
///   between `5` and `6.1` is `6.-1`; between `6.1` and `7` is `6.3`.
/// - With the same caret on both sides, keep it and decide on what
///   follows.
fn step_between(left: Option<&[i64]>, right: Option<&[i64]>) -> Vec<i64> {
    match (left, right) {
        (None, None) => vec![1],
        (Some(l), None) => vec![next_odd(l[0])],
        (None, Some(r)) => vec![previous_odd(r[0])],
        (Some(l), Some(r)) => {
            let (l0, r0) = (l[0], r[0]);
            debug_assert!(l0 <= r0, "left must be less than right");

            if l0 == r0 {
                // Both are inside the same caret.
                let mut step = vec![l0];
                step.extend(step_between(Some(&l[1..]), Some(&r[1..])));
                step
            } else if next_odd(l0) < r0 {
                vec![next_odd(l0)]
            } else if r0 - l0 >= 2 {
                // Both odd and two apart: the even ordinal between them.
                vec![l0 + 1, 1]
            } else if is_odd(l0) {
                // Right is inside caret `r0`; go in before it.
                let mut step = vec![r0];
                step.extend(step_between(None, Some(&r[1..])));
                step
            } else {
                // Left is inside caret `l0`; go in after it.
                let mut step = vec![l0];
                step.extend(step_between(Some(&l[1..]), None));
                step
            }
        }
    }
}

// ── Display ──────────────────────────────────────────────────────────

impl fmt::Display for HierarchyId {
    /// Canonical dotted form ("1.3.5"). The root prints as "/".
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.ordinals().next().is_none() {
            write!(f, "/")
        } else {
            fmt::Display::fmt(&self.inner, f)
        }
    }
}

impl fmt::Debug for HierarchyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HierarchyId({})", self)
    }
}

// ── Equality, ordering, hashing ──────────────────────────────────────

impl PartialEq for HierarchyId {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for HierarchyId {}

impl PartialOrd for HierarchyId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HierarchyId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner
            .partial_cmp(&other.inner)
            .unwrap_or(Ordering::Equal)
    }
}

impl Hash for HierarchyId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Public API tests ─────────────────────────────────────────────

    #[test]
    fn root_is_empty_and_displays_as_slash() {
        let root = HierarchyId::get_root();
        assert_eq!(root.get_level(), 0);
        assert_eq!(root.as_bytes().len(), 0);
        assert_eq!(root.to_string(), "/");
    }

    #[test]
    fn first_child_of_root() {
        let root = HierarchyId::get_root();
        let first = root.get_descendant(None, None);
        assert_eq!(first.get_level(), 1);
        assert_eq!(first.to_string(), "1");
    }

    #[test]
    fn first_child_of_inner_node() {
        let root = HierarchyId::get_root();
        let parent = root.get_descendant(None, None); // "1"
        let child = parent.get_descendant(None, None);
        assert_eq!(child.get_level(), 2);
        assert!(child > parent);
        assert!(child.is_descendant_of(&parent));
    }

    #[test]
    fn after_appends_sibling() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);
        let c = root.get_descendant(Some(&b), None);
        assert!(a < b && b < c);
        assert_eq!(b.get_level(), 1);
    }

    #[test]
    fn before_prepends_sibling() {
        let root = HierarchyId::get_root();
        let first = root.get_descendant(None, None);
        let earlier = root.get_descendant(None, Some(&first));
        assert!(earlier < first);

        let earlier2 = root.get_descendant(None, Some(&earlier));
        assert!(earlier2 < earlier);
    }

    #[test]
    fn between_inserts_strictly_in_gap() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);
        let mid = root.get_descendant(Some(&a), Some(&b));
        assert!(a < mid && mid < b);
    }

    #[test]
    fn between_handles_adjacent_via_caret() {
        // Force the caret path: insert repeatedly into the same gap.
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);

        let mut prev = a.clone();
        for _ in 0..10 {
            let mid = root.get_descendant(Some(&prev), Some(&b));
            assert!(prev < mid && mid < b);
            prev = mid;
        }
    }

    #[test]
    fn nested_descendants_preserve_subtree_order() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);
        let a1 = a.get_descendant(None, None);
        let a2 = a.get_descendant(Some(&a1), None);

        // Depth-first ordering: a < a1 < a2 < b
        assert!(a < a1);
        assert!(a1 < a2);
        assert!(a2 < b);
    }

    #[test]
    fn get_ancestor_walks_up() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let a1 = a.get_descendant(None, None);
        let a1x = a1.get_descendant(None, None);

        assert_eq!(a1x.get_ancestor(0).unwrap(), a1x);
        assert_eq!(a1x.get_ancestor(1).unwrap(), a1);
        assert_eq!(a1x.get_ancestor(2).unwrap(), a);
        assert_eq!(a1x.get_ancestor(3).unwrap(), root);
        assert!(a1x.get_ancestor(4).is_none());
        assert!(a1x.get_ancestor(-1).is_none());
    }

    #[test]
    fn is_descendant_of_relationships() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let a1 = a.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);

        assert!(a1.is_descendant_of(&a));
        assert!(a1.is_descendant_of(&root));
        assert!(!a.is_descendant_of(&b));
        assert!(!a.is_descendant_of(&a)); // not descendant of self
    }

    #[test]
    fn bytes_round_trip() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let a1 = a.get_descendant(None, None);

        let bytes = a1.to_vec();
        let restored = HierarchyId::from_bytes(&bytes);
        assert_eq!(a1, restored);
        assert_eq!(a1.as_bytes(), restored.as_bytes());
    }

    #[test]
    fn ordering_is_total_and_byte_based() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);
        let a1 = a.get_descendant(None, None);

        let mut v = vec![b.clone(), a1.clone(), a.clone(), root.clone()];
        v.sort();
        assert_eq!(v, vec![root, a, a1, b]);
    }

    #[test]
    fn debug_format_includes_dotted_form() {
        let root = HierarchyId::get_root();
        assert_eq!(format!("{:?}", root), "HierarchyId(/)");
        let a = root.get_descendant(None, None);
        assert_eq!(format!("{:?}", a), "HierarchyId(1)");
    }

    // ── Internal-API tests (ordinal-aware, in-module access) ─────────

    #[test]
    fn from_ordinals_round_trip() {
        let h = HierarchyId::from_ordinals(&[1, 3, 5]);
        assert_eq!(h.ordinals(), vec![1, 3, 5]);
        assert_eq!(h.to_string(), "1.3.5");
    }

    #[test]
    fn child_of_appends_ordinal() {
        let parent = HierarchyId::from_ordinals(&[1, 3]);
        let child = HierarchyId::child_of(&parent, 5);
        assert_eq!(child.ordinals(), vec![1, 3, 5]);
    }

    #[test]
    fn paper_caret_example() {
        // ORDPATH paper §3.3: between 3.5.5 and 3.5.7, caret 6 gives 3.5.6.1.
        let parent = HierarchyId::from_ordinals(&[3, 5]);
        let l = HierarchyId::from_ordinals(&[3, 5, 5]);
        let r = HierarchyId::from_ordinals(&[3, 5, 7]);
        let mid = parent.get_descendant(Some(&l), Some(&r));
        assert_eq!(mid.ordinals(), vec![3, 5, 6, 1]);
    }

    #[test]
    fn paper_negative_ordinals() {
        // Inserting before [1] yields [-1]; before [-1] yields [-3].
        let root = HierarchyId::get_root();
        let first = root.get_descendant(None, None);
        let neg = root.get_descendant(None, Some(&first));
        assert_eq!(neg.ordinals(), vec![-1]);
        let neg2 = root.get_descendant(None, Some(&neg));
        assert_eq!(neg2.ordinals(), vec![-3]);
    }

    #[test]
    fn terminal_ordinal_is_always_odd() {
        let root = HierarchyId::get_root();
        let a = root.get_descendant(None, None);
        let b = root.get_descendant(Some(&a), None);
        let mid = root.get_descendant(Some(&a), Some(&b));

        for h in [&a, &b, &mid] {
            assert!(is_odd(*h.ordinals().last().unwrap()), "{:?}", h);
        }
    }

    #[test]
    fn a_new_sibling_is_never_placed_beneath_its_neighbor() {
        let parent = HierarchyId::from_ordinals(&[3]);
        let between = |l: &[i64], r: &[i64]| {
            parent
                .get_descendant(
                    Some(&HierarchyId::from_ordinals(l)),
                    Some(&HierarchyId::from_ordinals(r)),
                )
                .ordinals()
        };

        // Room for an odd ordinal, then only for a caret.
        assert_eq!(between(&[3, 3], &[3, 7]), vec![3, 5]);
        assert_eq!(between(&[3, 5], &[3, 7]), vec![3, 6, 1]);
        // Next to a caret-inserted sibling: go inside the caret. The
        // old answers here were 3.5.2.1 and 3.6.1.2.1, which read as
        // children of 3.5 and of 3.6.1.
        assert_eq!(between(&[3, 5], &[3, 6, 1]), vec![3, 6, -1]);
        assert_eq!(between(&[3, 6, 1], &[3, 7]), vec![3, 6, 3]);
        // Inside one caret.
        assert_eq!(between(&[3, 6, 1], &[3, 6, 3]), vec![3, 6, 2, 1]);
        assert_eq!(between(&[3, 6, 1], &[3, 6, 2, 1]), vec![3, 6, 2, -1]);
        assert_eq!(between(&[3, 6, -1], &[3, 6, 1]), vec![3, 6, 0, 1]);
    }

    #[test]
    fn levels_and_ancestors_skip_carets() {
        let node = HierarchyId::from_ordinals(&[3, 6, 2, 1, 5]);
        assert_eq!(node.get_level(), 3);
        assert_eq!(node.get_ancestor(1).unwrap().ordinals(), vec![3, 6, 2, 1]);
        assert_eq!(node.get_ancestor(2).unwrap().ordinals(), vec![3]);
        assert_eq!(node.get_ancestor(3).unwrap(), HierarchyId::get_root());
        assert!(node.get_ancestor(4).is_none());
    }

    /// Build a tree by inserting nodes at random places, as file creation
    /// does, and check the three properties everything else relies on:
    /// positions are unique, siblings sort in their intended order, and a
    /// node's byte range holds exactly its descendants.
    #[test]
    fn random_insertions_keep_ids_unique_ordered_and_nested() {
        struct Node {
            id: HierarchyId,
            parent: Option<usize>,
            /// Child indexes in intended order.
            children: Vec<usize>,
        }

        // Small deterministic generator; the test must not be flaky.
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut random = move |bound: usize| -> usize {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % bound
        };

        let mut nodes: Vec<Node> = Vec::new();
        let mut top_level: Vec<usize> = Vec::new();

        for _ in 0..4000 {
            // Pick a parent: the root, or an existing node. Favor a few
            // parents so the same gaps are split again and again.
            let parent = match random(4) {
                0 => None,
                _ if nodes.is_empty() => None,
                1 => Some(random(nodes.len().min(5))),
                _ => Some(random(nodes.len())),
            };
            let parent_id = match parent {
                Some(p) => nodes[p].id.clone(),
                None => HierarchyId::get_root(),
            };
            let siblings = match parent {
                Some(p) => nodes[p].children.clone(),
                None => top_level.clone(),
            };

            // Insert at a random place among the siblings, with a bias
            // toward the front and toward one repeated spot.
            let at = match random(3) {
                0 => 0,
                1 => siblings.len().min(1),
                _ => random(siblings.len() + 1),
            };
            let left = at.checked_sub(1).map(|i| &nodes[siblings[i]].id);
            let right = siblings.get(at).map(|i| &nodes[*i].id);
            let id = parent_id.get_descendant(left, right);

            let index = nodes.len();
            nodes.push(Node {
                id,
                parent,
                children: Vec::new(),
            });
            match parent {
                Some(p) => nodes[p].children.insert(at, index),
                None => top_level.insert(at, index),
            }
        }

        // Unique.
        let mut bytes: Vec<&[u8]> = nodes.iter().map(|n| n.id.as_bytes()).collect();
        bytes.sort();
        let before = bytes.len();
        bytes.dedup();
        assert_eq!(bytes.len(), before, "two nodes share a position");

        // Siblings sort in their intended order.
        let mut sibling_lists: Vec<&Vec<usize>> = nodes.iter().map(|n| &n.children).collect();
        sibling_lists.push(&top_level);
        for list in sibling_lists {
            for pair in list.windows(2) {
                assert!(nodes[pair[0]].id < nodes[pair[1]].id);
            }
        }

        // A node's byte range holds exactly its descendants, and levels
        // and parents can be recovered from the position alone.
        let is_beneath = |mut node: usize, ancestor: usize| -> bool {
            while let Some(parent) = nodes[node].parent {
                if parent == ancestor {
                    return true;
                }
                node = parent;
            }
            false
        };
        for (a, ancestor) in nodes.iter().enumerate().step_by(7) {
            let lower = ancestor.id.as_bytes();
            let upper = ancestor.id.subtree_end().unwrap();
            for (n, node) in nodes.iter().enumerate() {
                let in_range = node.id.as_bytes() > lower && node.id.as_bytes() < upper.as_slice();
                assert_eq!(in_range, is_beneath(n, a), "{:?} vs {:?}", node.id, ancestor.id);
                assert_eq!(node.id.is_descendant_of(&ancestor.id), is_beneath(n, a));
            }
        }
        for node in &nodes {
            let parent_id = match node.parent {
                Some(p) => nodes[p].id.clone(),
                None => HierarchyId::get_root(),
            };
            assert_eq!(node.id.get_ancestor(1).unwrap(), parent_id);
            assert_eq!(node.id.get_level(), parent_id.get_level() + 1);
        }
    }
}
