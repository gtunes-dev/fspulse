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
    pub fn get_level(&self) -> i32 {
        self.inner.ordinals().count() as i32
    }

    /// Returns the ancestor `n` levels above this node, or `None` if `n`
    /// exceeds this node's level. `get_ancestor(0)` returns a clone of self;
    /// `get_ancestor(1)` returns the immediate parent.
    pub fn get_ancestor(&self, n: i32) -> Option<HierarchyId> {
        if n < 0 {
            return None;
        }
        let ords = self.ordinals();
        let n = n as usize;
        if n > ords.len() {
            None
        } else {
            Some(HierarchyId::from_ordinals(&ords[..ords.len() - n]))
        }
    }

    /// Returns a new descendant of `self` whose position sorts strictly
    /// between `child1` and `child2`.
    ///
    /// - `(None, None)` — first child of `self`.
    /// - `(Some(l), None)` — sibling immediately after `l`.
    /// - `(None, Some(r))` — sibling immediately before `r`.
    /// - `(Some(l), Some(r))` — sibling strictly between `l` and `r`.
    ///
    /// When supplied, `child1` and `child2` must be descendants of `self`
    /// (typically direct children, but caret-inserted positions can sit
    /// deeper in the subtree and remain valid arguments here), and
    /// `child1 < child2` must hold. Violations are caught by `debug_assert`
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

        match (child1, child2) {
            (None, None) => Self::child_of(self, 1),
            (Some(l), None) => Self::insert_after(l),
            (None, Some(r)) => Self::insert_before(r),
            (Some(l), Some(r)) => Self::insert_between_impl(l, r),
        }
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

    /// Sibling immediately after `left` (bumps the last ordinal by 2 to
    /// stay on odd terminals).
    fn insert_after(left: &HierarchyId) -> HierarchyId {
        let mut ords = left.ordinals();
        let last = ords.last_mut().expect("cannot insert after root");
        *last += 2;
        HierarchyId::from_ordinals(&ords)
    }

    /// Sibling immediately before `right` (decrements the last ordinal by 2;
    /// negative ordinals are valid and sort before positive ones).
    fn insert_before(right: &HierarchyId) -> HierarchyId {
        let mut ords = right.ordinals();
        let last = ords.last_mut().expect("cannot insert before root");
        *last -= 2;
        HierarchyId::from_ordinals(&ords)
    }

    /// Insert between two sibling positions, using the ORDPATH even-caret
    /// mechanism (O'Neil et al., SIGMOD 2004 §3.3) when no odd ordinal fits
    /// directly between the divergence point.
    fn insert_between_impl(left: &HierarchyId, right: &HierarchyId) -> HierarchyId {
        let l_ords = left.ordinals();
        let r_ords = right.ordinals();

        let common = l_ords
            .iter()
            .zip(r_ords.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common == l_ords.len() && common < r_ords.len() {
            // Left is a strict prefix of right: e.g. left=[3], right=[3,4,1].
            // Place an even caret < right's next ordinal, then odd 1.
            let r_next = r_ords[common];
            let caret = if r_next % 2 == 0 { r_next - 2 } else { r_next - 1 };
            let mut result = l_ords.to_vec();
            result.push(caret);
            result.push(1);
            HierarchyId::from_ordinals(&result)
        } else if common == r_ords.len() && common <= l_ords.len() {
            panic!("left must be less than right");
        } else {
            let l_val = l_ords[common];
            let r_val = r_ords[common];
            debug_assert!(l_val < r_val, "left must be less than right");
            Self::insert_at_divergence(&l_ords[..common], &l_ords, l_val, r_val)
        }
    }

    /// Resolve the divergence between two siblings: prefer an odd ordinal in
    /// the gap; otherwise drop a caret and recurse with odd 1; otherwise
    /// extend left's full path with caret 2 + odd 1.
    fn insert_at_divergence(
        prefix: &[i64],
        l_ords: &[i64],
        l_val: i64,
        r_val: i64,
    ) -> HierarchyId {
        let next_odd = if l_val % 2 == 0 { l_val + 1 } else { l_val + 2 };

        if next_odd < r_val {
            let mut result = prefix.to_vec();
            result.push(next_odd);
            HierarchyId::from_ordinals(&result)
        } else if r_val - l_val >= 2 {
            let caret = l_val + 1;
            let mut result = prefix.to_vec();
            result.push(caret);
            result.push(1);
            HierarchyId::from_ordinals(&result)
        } else {
            let mut result = l_ords.to_vec();
            result.push(2);
            result.push(1);
            HierarchyId::from_ordinals(&result)
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
            assert_eq!(h.ordinals().last().unwrap() % 2, 1, "{:?}", h);
        }
    }
}
