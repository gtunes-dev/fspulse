use std::cmp::Ordering;
use std::fmt;

use super::ordpath::{DefaultEncoding, OrdPathBuf};

/// A hierarchical position identifier for filesystem tree nodes.
/// Wraps an OrdPath-encoded byte sequence. Byte comparison gives
/// depth-first tree ordering; prefix matching gives ancestor/descendant.
#[derive(Clone)]
pub struct HierarchyId {
    inner: OrdPathBuf<DefaultEncoding>,
}

impl HierarchyId {
    // ── Construction ──────────────────────────────────────────────

    /// Create a HierarchyId for a root-level child at the given ordinal position.
    /// Ordinals should be odd (1, 3, 5, ...) to leave room for insertions.
    pub fn new_child(ordinal: i64) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_ordinals(&[ordinal], DefaultEncoding),
        }
    }

    /// Create a HierarchyId for a child of `parent` at the given ordinal position.
    pub fn child_of(parent: &HierarchyId, ordinal: i64) -> HierarchyId {
        let mut ords = parent.ordinals();
        ords.push(ordinal);
        HierarchyId {
            inner: OrdPathBuf::from_ordinals(&ords, DefaultEncoding),
        }
    }

    /// Create from a sequence of ordinals (e.g., [1, 3, 5] for a 3-deep path).
    pub fn from_ordinals(ordinals: &[i64]) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_ordinals(ordinals, DefaultEncoding),
        }
    }

    /// Create from raw bytes (as stored in SQLite BLOB column).
    pub fn from_bytes(bytes: &[u8]) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_bytes(bytes, DefaultEncoding),
        }
    }

    /// Create from dot-separated string (e.g., "1.3.5") -- useful for debugging.
    pub fn from_str(s: &str) -> HierarchyId {
        HierarchyId {
            inner: OrdPathBuf::from_str(s, DefaultEncoding),
        }
    }

    // ── Insert Between ───────────────────────────────────────────

    /// Compute a HierarchyId that sorts between `left` and `right`.
    /// Both must be siblings (same parent). Returns a HierarchyId h where
    /// left < h < right in byte ordering.
    ///
    /// Uses the OrdPath "caret" mechanism (O'Neil et al., SIGMOD 2004):
    /// - Initial siblings use odd ordinals: 1, 3, 5, 7, ...
    /// - Even ordinals are reserved as non-terminal "carets" for insertions.
    /// - To insert between siblings 5 and 7, use even caret 6 followed by
    ///   odd 1: the result is parent.6.1 (not parent.5.6.1).
    /// - All node labels always end in an odd ordinal component.
    ///
    /// If `left` is None, insert before `right` (subtracts 2 from last ordinal).
    /// If `right` is None, insert after `left` (adds 2 to last ordinal).
    pub fn insert_between(
        left: Option<&HierarchyId>,
        right: Option<&HierarchyId>,
    ) -> HierarchyId {
        match (left, right) {
            (Some(l), Some(r)) => Self::insert_between_impl(l, r),
            (Some(l), None) => Self::insert_after(l),
            (None, Some(r)) => Self::insert_before(r),
            (None, None) => panic!("insert_between requires at least one bound"),
        }
    }

    /// Insert after `left` with no right bound.
    /// Take left's ordinals and bump the last ordinal by 2 (keep odd).
    fn insert_after(left: &HierarchyId) -> HierarchyId {
        let mut ords = left.ordinals();
        assert!(!ords.is_empty(), "cannot insert after empty path");
        let last = ords.last_mut().unwrap();
        *last += 2;
        HierarchyId::from_ordinals(&ords)
    }

    /// Insert before `right` with no left bound.
    /// Decrement the last ordinal by 2. This works even for ordinal 1,
    /// producing ordinal -1 which encodes to a byte sequence that sorts before 1.
    fn insert_before(right: &HierarchyId) -> HierarchyId {
        let mut ords = right.ordinals();
        assert!(!ords.is_empty(), "cannot insert before empty path");
        *ords.last_mut().unwrap() -= 2;
        HierarchyId::from_ordinals(&ords)
    }

    /// Insert between two siblings.
    ///
    /// Follows the ORDPATH paper (O'Neil et al., SIGMOD 2004) Section 3.3:
    /// - All node labels end in an odd ordinal component.
    /// - Even ordinals are non-terminal "carets" used only for insertions.
    /// - Between siblings with final odd ordinals L and R, use an even
    ///   caret E (where L < E < R) followed by odd 1.
    fn insert_between_impl(left: &HierarchyId, right: &HierarchyId) -> HierarchyId {
        let l_ords = left.ordinals();
        let r_ords = right.ordinals();

        // Find common ordinal prefix length.
        let common = l_ords
            .iter()
            .zip(r_ords.iter())
            .take_while(|(a, b)| a == b)
            .count();

        if common == l_ords.len() && common < r_ords.len() {
            // Left's ordinals are a strict prefix of right's.
            // e.g. left=[3], right=[3,4,1]
            // Insert [prefix, caret, 1] where caret is even and < right's next ordinal.
            let r_next = r_ords[common];
            let caret = if r_next % 2 == 0 { r_next - 2 } else { r_next - 1 };
            let mut result = l_ords.to_vec();
            result.push(caret);
            result.push(1);
            HierarchyId::from_ordinals(&result)
        } else if common == r_ords.len() && common <= l_ords.len() {
            // Right's ordinals are a prefix of left's — this means right < left,
            // which violates the precondition.
            panic!("left must be less than right");
        } else {
            // Both have ordinals at the divergence point.
            let l_val = l_ords[common];
            let r_val = r_ords[common];
            debug_assert!(l_val < r_val, "left must be less than right");
            Self::insert_at_divergence(&l_ords[..common], &l_ords, l_val, r_val)
        }
    }

    /// Insert between two ordinal values at a divergence point.
    ///
    /// Per the ORDPATH paper:
    /// 1. If an odd ordinal fits between l_val and r_val, use it directly.
    /// 2. If the gap is exactly 2 (adjacent odds like 1,3): use even caret
    ///    between them, then odd 1. E.g. between 5 and 7 → [prefix, 6, 1].
    /// 3. If the gap is 1 (e.g. l_val=3, r_val=4 from a prior caret): extend
    ///    left's full ordinal path with [2, 1].
    fn insert_at_divergence(
        prefix: &[i64],
        l_ords: &[i64],
        l_val: i64,
        r_val: i64,
    ) -> HierarchyId {
        // Try to find an odd ordinal between l_val and r_val.
        let next_odd = if l_val % 2 == 0 { l_val + 1 } else { l_val + 2 };

        if next_odd < r_val {
            // An odd ordinal fits — use it directly at this level.
            let mut result = prefix.to_vec();
            result.push(next_odd);
            HierarchyId::from_ordinals(&result)
        } else if r_val - l_val >= 2 {
            // Gap of exactly 2 (adjacent odds, e.g. 1 and 3).
            // Per paper Section 3.3: even caret between them, then odd 1.
            let caret = l_val + 1;
            let mut result = prefix.to_vec();
            result.push(caret);
            result.push(1);
            HierarchyId::from_ordinals(&result)
        } else {
            // Gap of 1 (e.g. l_val=3, r_val=4 where 4 is an even caret in right).
            // Extend left's full ordinal path with even caret 2 + odd 1.
            // This sorts after left (prefix extension) and before right (l_val < r_val).
            let mut result = l_ords.to_vec();
            result.push(2);
            result.push(1);
            HierarchyId::from_ordinals(&result)
        }
    }

    // ── Navigation ───────────────────────────────────────────────

    /// Get the parent's HierarchyId (all ordinals except the last).
    /// Returns None for root-level items (single ordinal).
    pub fn parent(&self) -> Option<HierarchyId> {
        let ords = self.ordinals();
        if ords.len() <= 1 {
            None
        } else {
            Some(HierarchyId::from_ordinals(&ords[..ords.len() - 1]))
        }
    }

    /// Get the depth (number of ordinal components). Root children = 1.
    pub fn depth(&self) -> usize {
        self.inner.ordinals().count()
    }

    /// True if `self` is an ancestor of `other` (other is in self's subtree).
    pub fn is_ancestor_of(&self, other: &HierarchyId) -> bool {
        self.inner.is_ancestor_of(&other.inner)
    }

    /// True if `self` is a descendant of `other`.
    pub fn is_descendant_of(&self, other: &HierarchyId) -> bool {
        self.inner.is_descendant_of(&other.inner)
    }

    /// Get the ordinal components as a Vec<i64>.
    pub fn ordinals(&self) -> Vec<i64> {
        self.inner.ordinals().collect()
    }

    // ── Byte Access ──────────────────────────────────────────────

    /// Get the raw bytes for storage in a BLOB column.
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.as_ref()
    }

    /// Convert to owned Vec<u8>.
    pub fn to_vec(&self) -> Vec<u8> {
        self.inner.as_ref().to_vec()
    }
}

// ── Display ──────────────────────────────────────────────────────

impl fmt::Display for HierarchyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl fmt::Debug for HierarchyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HierarchyId({})", self)
    }
}

// ── Ordering ─────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_child() {
        let h = HierarchyId::new_child(1);
        assert_eq!(h.ordinals(), vec![1]);
        assert_eq!(h.depth(), 1);
    }

    #[test]
    fn test_child_of() {
        let parent = HierarchyId::new_child(1);
        let child = HierarchyId::child_of(&parent, 3);
        assert_eq!(child.ordinals(), vec![1, 3]);
        assert_eq!(child.depth(), 2);
    }

    #[test]
    fn test_from_ordinals_roundtrip() {
        let ords = &[1, 3, 5];
        let h = HierarchyId::from_ordinals(ords);
        assert_eq!(h.ordinals(), ords);
    }

    #[test]
    fn test_from_bytes_roundtrip() {
        let h1 = HierarchyId::from_ordinals(&[1, 3, 5]);
        let h2 = HierarchyId::from_bytes(h1.as_bytes());
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_from_str() {
        let h = HierarchyId::from_str("1.3.5");
        assert_eq!(h.ordinals(), vec![1, 3, 5]);
    }

    #[test]
    fn test_display() {
        let h = HierarchyId::from_ordinals(&[1, 3, 5]);
        assert_eq!(h.to_string(), "1.3.5");
    }

    #[test]
    fn test_depth_first_ordering() {
        // Tree: A(1), A/B(1.1), A/B/C(1.1.1), A/D(1.3), E(3)
        let a = HierarchyId::from_ordinals(&[1]);
        let ab = HierarchyId::from_ordinals(&[1, 1]);
        let abc = HierarchyId::from_ordinals(&[1, 1, 1]);
        let ad = HierarchyId::from_ordinals(&[1, 3]);
        let e = HierarchyId::from_ordinals(&[3]);

        assert!(a < ab);
        assert!(ab < abc);
        assert!(abc < ad);
        assert!(ad < e);

        // Also verify sort
        let mut items = vec![e.clone(), abc.clone(), a.clone(), ad.clone(), ab.clone()];
        items.sort();
        assert_eq!(items, vec![a, ab, abc, ad, e]);
    }

    #[test]
    fn test_ancestor_detection() {
        let a = HierarchyId::from_ordinals(&[1]);
        let ab = HierarchyId::from_ordinals(&[1, 3]);
        let abc = HierarchyId::from_ordinals(&[1, 3, 5]);
        let e = HierarchyId::from_ordinals(&[3]);

        assert!(a.is_ancestor_of(&ab));
        assert!(a.is_ancestor_of(&abc));
        assert!(ab.is_ancestor_of(&abc));
        assert!(!a.is_ancestor_of(&e));
        assert!(!ab.is_ancestor_of(&e));
        assert!(!a.is_ancestor_of(&a)); // not ancestor of self

        assert!(abc.is_descendant_of(&a));
        assert!(abc.is_descendant_of(&ab));
        assert!(!e.is_descendant_of(&a));
    }

    #[test]
    fn test_parent_navigation() {
        let h = HierarchyId::from_ordinals(&[1, 3, 5]);
        let parent = h.parent().unwrap();
        assert_eq!(parent.ordinals(), vec![1, 3]);

        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent.ordinals(), vec![1]);

        assert!(grandparent.parent().is_none());
    }

    #[test]
    fn test_child_of_parent_roundtrip() {
        let parent = HierarchyId::from_ordinals(&[1, 3]);
        let child = HierarchyId::child_of(&parent, 5);
        assert_eq!(child.parent().unwrap(), parent);
    }

    #[test]
    fn test_insert_between_with_gap() {
        // Between ordinals 1 and 5 — room for 2, 3, or 4
        let left = HierarchyId::new_child(1);
        let right = HierarchyId::new_child(5);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));

        assert!(left < mid, "left < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < right: {} < {}", mid, right);
    }

    #[test]
    fn test_insert_between_adjacent_odds() {
        // Between 1 and 3 — no integer between them at same level, must use caret
        let left = HierarchyId::new_child(1);
        let right = HierarchyId::new_child(3);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));

        assert!(left < mid, "left < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < right: {} < {}", mid, right);
    }

    #[test]
    fn test_insert_between_with_parent() {
        // Siblings under a common parent
        let parent = HierarchyId::from_ordinals(&[1]);
        let left = HierarchyId::child_of(&parent, 1);
        let right = HierarchyId::child_of(&parent, 3);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));

        assert!(left < mid, "left < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < right: {} < {}", mid, right);
    }

    #[test]
    fn test_insert_after() {
        let left = HierarchyId::new_child(3);
        let after = HierarchyId::insert_between(Some(&left), None);
        assert!(left < after, "{} < {}", left, after);
    }

    #[test]
    fn test_insert_before() {
        let right = HierarchyId::new_child(5);
        let before = HierarchyId::insert_between(None, Some(&right));
        assert!(before < right, "{} < {}", before, right);
    }

    #[test]
    fn test_insert_before_small_ordinal() {
        let right = HierarchyId::new_child(1);
        let before = HierarchyId::insert_between(None, Some(&right));
        assert!(before < right, "{} < {}", before, right);
    }

    #[test]
    fn test_repeated_insertions_same_gap() {
        // Repeatedly insert between left and the last inserted value
        let left = HierarchyId::new_child(1);
        let right = HierarchyId::new_child(3);

        let mut prev = left.clone();
        let bound = right.clone();
        for i in 0..10 {
            let mid = HierarchyId::insert_between(Some(&prev), Some(&bound));
            assert!(
                prev < mid,
                "iteration {}: prev < mid: {} < {}",
                i,
                prev,
                mid
            );
            assert!(
                mid < bound,
                "iteration {}: mid < bound: {} < {}",
                i,
                mid,
                bound
            );
            prev = mid;
        }
    }

    #[test]
    fn test_repeated_insertions_at_start() {
        // Repeatedly insert before the first element
        let mut first = HierarchyId::new_child(1);
        for i in 0..10 {
            let before = HierarchyId::insert_between(None, Some(&first));
            assert!(
                before < first,
                "iteration {}: before < first: {} < {}",
                i,
                before,
                first
            );
            first = before;
        }
    }

    #[test]
    fn test_insert_between_different_depths() {
        // left=[3], right=[3,4,1] — left is shorter
        let left = HierarchyId::from_ordinals(&[3]);
        let right = HierarchyId::from_ordinals(&[3, 4, 1]);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert!(left < mid, "left < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < right: {} < {}", mid, right);
    }

    #[test]
    fn test_insert_between_left_deeper() {
        // left=[3,4,1], right=[5]
        let left = HierarchyId::from_ordinals(&[3, 4, 1]);
        let right = HierarchyId::from_ordinals(&[5]);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert!(left < mid, "left < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < right: {} < {}", mid, right);
    }

    #[test]
    fn test_to_vec() {
        let h = HierarchyId::from_ordinals(&[1, 3]);
        let v = h.to_vec();
        assert_eq!(v, h.as_bytes().to_vec());
    }

    #[test]
    fn test_ordinals_roundtrip() {
        let h1 = HierarchyId::from_ordinals(&[1, 3, 5, 7]);
        let h2 = HierarchyId::from_ordinals(&h1.ordinals());
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_depth() {
        assert_eq!(HierarchyId::from_ordinals(&[1]).depth(), 1);
        assert_eq!(HierarchyId::from_ordinals(&[1, 3]).depth(), 2);
        assert_eq!(HierarchyId::from_ordinals(&[1, 3, 5]).depth(), 3);
    }

    #[test]
    fn test_insert_between_produces_odd_terminal() {
        // ORDPATH paper invariant: all node labels end in an odd ordinal.
        #[allow(clippy::type_complexity)]
        let cases: Vec<(Option<Vec<i64>>, Option<Vec<i64>>)> = vec![
            (Some(vec![1]), Some(vec![3])),       // adjacent odds
            (Some(vec![1]), Some(vec![5])),       // gap with room
            (Some(vec![3]), None),                // insert after
            (None, Some(vec![5])),                // insert before
            (None, Some(vec![1])),                // insert before small
            (Some(vec![3]), Some(vec![3, 4, 1])), // left is prefix
            (Some(vec![3, 4, 1]), Some(vec![5])), // left is deeper
            (Some(vec![1, 1]), Some(vec![1, 3])), // with parent, adjacent
        ];

        for (l, r) in &cases {
            let left = l.as_ref().map(|o| HierarchyId::from_ordinals(o));
            let right = r.as_ref().map(|o| HierarchyId::from_ordinals(o));
            let result = HierarchyId::insert_between(
                left.as_ref(),
                right.as_ref(),
            );
            let ords = result.ordinals();
            let last = ords.last().expect("result should not be empty");
            assert!(
                last % 2 != 0,
                "Result {} has even terminal ordinal {} (left={:?}, right={:?})",
                result, last, l, r
            );
        }
    }

    #[test]
    fn test_paper_caret_examples() {
        // From ORDPATH paper Section 3.3:
        // Between siblings 3.5.5 and 3.5.7, caret 6 gives 3.5.6.1
        let left = HierarchyId::from_ordinals(&[3, 5, 5]);
        let right = HierarchyId::from_ordinals(&[3, 5, 7]);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert_eq!(mid.ordinals(), vec![3, 5, 6, 1]);
        assert!(left < mid);
        assert!(mid < right);

        // A sequence of careted-in siblings: 3.5.6.1, 3.5.6.3, 3.5.6.5
        let s1 = HierarchyId::from_ordinals(&[3, 5, 6, 1]);
        let s2 = HierarchyId::from_ordinals(&[3, 5, 6, 3]);
        let s3 = HierarchyId::from_ordinals(&[3, 5, 6, 5]);
        assert!(left < s1);
        assert!(s1 < s2);
        assert!(s2 < s3);
        assert!(s3 < right);
    }

    #[test]
    fn test_paper_negative_ordinals() {
        // From ORDPATH paper Section 3.3:
        // Insert before existing children by subtracting 2, using negative ordinals.
        let first = HierarchyId::new_child(1);
        let before = HierarchyId::insert_between(None, Some(&first));
        assert_eq!(before.ordinals(), vec![-1]);
        assert!(before < first);

        // Can keep going negative
        let before2 = HierarchyId::insert_between(None, Some(&before));
        assert_eq!(before2.ordinals(), vec![-3]);
        assert!(before2 < before);
    }

    #[test]
    fn test_repeated_caret_insertions() {
        // Repeatedly insert between the same pair, accumulating carets.
        // Per the paper, multiple caret levels are rare but must work.
        let left = HierarchyId::new_child(1);
        let right = HierarchyId::new_child(3);

        // First insert: between [1] and [3] → [2, 1]
        let m1 = HierarchyId::insert_between(Some(&left), Some(&right));
        assert_eq!(m1.ordinals(), vec![2, 1]);
        assert!(left < m1 && m1 < right);

        // Insert between [1] and [2, 1] → [1, 2, 1] (extends left with caret)
        // Wait - this goes through the "left prefix" case since [1] is a prefix...
        // Actually [1] vs [2, 1]: diverge at position 0, l_val=1, r_val=2.
        // Gap=1, extends left: [1, 2, 1].
        let m2 = HierarchyId::insert_between(Some(&left), Some(&m1));
        assert!(left < m2 && m2 < m1, "{} < {} < {}", left, m2, m1);

        // Insert between [2, 1] and [3] → [2, 1] vs [3], diverge at 0: 2 vs 3.
        // next_odd = 3 (2 is even, +1=3). 3 < 3? No. Gap = 1.
        // Extend left: [2, 1, 2, 1].
        let m3 = HierarchyId::insert_between(Some(&m1), Some(&right));
        assert!(m1 < m3 && m3 < right, "{} < {} < {}", m1, m3, right);

        // All results end in odd
        for h in [&m1, &m2, &m3] {
            assert!(h.ordinals().last().unwrap() % 2 != 0);
        }
    }

    #[test]
    fn test_insert_between_negative_ordinals() {
        // Between two negative odds: [-5] and [-3]
        let left = HierarchyId::new_child(-5);
        let right = HierarchyId::new_child(-3);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert!(left < mid, "[-5] < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < [-3]: {} < {}", mid, right);
        assert!(mid.ordinals().last().unwrap() % 2 != 0, "terminal must be odd");

        // Between negative and positive: [-1] and [1]
        let left = HierarchyId::new_child(-1);
        let right = HierarchyId::new_child(1);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert!(left < mid, "[-1] < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < [1]: {} < {}", mid, right);
        assert!(mid.ordinals().last().unwrap() % 2 != 0, "terminal must be odd");

        // Between negative and positive with gap: [-3] and [5]
        let left = HierarchyId::new_child(-3);
        let right = HierarchyId::new_child(5);
        let mid = HierarchyId::insert_between(Some(&left), Some(&right));
        assert!(left < mid, "[-3] < mid: {} < {}", left, mid);
        assert!(mid < right, "mid < [5]: {} < {}", mid, right);
        assert!(mid.ordinals().last().unwrap() % 2 != 0, "terminal must be odd");

        // Repeated insertions in negative range
        let mut prev = HierarchyId::new_child(-7);
        let bound = HierarchyId::new_child(-5);
        for i in 0..5 {
            let mid = HierarchyId::insert_between(Some(&prev), Some(&bound));
            assert!(prev < mid, "iter {}: {} < {}", i, prev, mid);
            assert!(mid < bound, "iter {}: {} < {}", i, mid, bound);
            assert!(mid.ordinals().last().unwrap() % 2 != 0, "terminal must be odd");
            prev = mid;
        }
    }

    #[test]
    fn test_insert_between_after_left_insertions() {
        // Simulate: initial children [1, 3, 5], then insert-before creates [-1],
        // then insert between [-1] and [1].
        let neg1 = HierarchyId::new_child(-1);
        let pos1 = HierarchyId::new_child(1);
        let mid = HierarchyId::insert_between(Some(&neg1), Some(&pos1));
        assert!(neg1 < mid, "{} < {}", neg1, mid);
        assert!(mid < pos1, "{} < {}", mid, pos1);
        assert!(mid.ordinals().last().unwrap() % 2 != 0, "terminal must be odd");
    }

    #[test]
    #[should_panic]
    fn test_insert_between_none_none() {
        HierarchyId::insert_between(None, None);
    }
}
