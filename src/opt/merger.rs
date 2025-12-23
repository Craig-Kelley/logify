use std::marker::PhantomData;

use hashbrown::HashMap;

use bitflags::bitflags;

use crate::expr::{Expression, Node, NodeId};

bitflags! {
    #[derive(Clone, Copy, PartialEq)]
    pub(crate) struct MergeRelation: u8 {
        const TRIVIAL	= 0; // A and B are not related

        const SUBSET	= 0b0001; // A sub B
        const SUPERSET	= 0b0010; // A sup B
        const DISJOINT	= 0b0100; // A disjoint B
        const COVER		= 0b1000; // A | B == Universal

        const EQUAL			= Self::SUBSET.bits() | Self::SUPERSET.bits(); // (A sub B) and (A sup B)
        const COMPLEMENTARY	= Self::DISJOINT.bits() | Self::COVER.bits(); // (A disj B) and (A | B == Universal)
    }
}

impl MergeRelation {
    pub(crate) fn flip(self) -> Self {
        match self {
            MergeRelation::SUBSET => MergeRelation::SUPERSET,
            MergeRelation::SUPERSET => MergeRelation::SUBSET,
            _ => self,
        }
    }

    // simple checks
    pub(crate) fn is_subset(&self) -> bool {
        self.contains(Self::SUBSET)
    }
    pub(crate) fn is_superset(&self) -> bool {
        self.contains(Self::SUPERSET)
    }
    pub(crate) fn is_disjoint(&self) -> bool {
        self.contains(Self::DISJOINT)
    }
    pub(crate) fn is_cover(&self) -> bool {
        self.contains(Self::COVER)
    }
}

/// Describes how two sets relate to one another.
///
/// This is returned by [`Mergeable::get_relation`]. The optimizer uses this to remove
/// redundant logic (e.g., if `A` is a subset of `B`, then `A & B` simplifies to `A`).
///
/// # Hierarchy
/// Return the most specific relationship possible.
/// 1. **Equal:** Sets are identical.
/// 2. **Subset / Superset:** One set strictly contains the other.
/// 3. **Complementary:** Sets are disjoint AND fill the universe.
/// 4. **Cover:** Union fills the universe.
/// 5. **Disjoint:** Intersection is empty.
/// 6. **Trivial:** No special relationship.
///
/// One or more results can be left out of the return. However, it may prevent optimizations.
///
/// **Subet / Superset** depend on each other, so returning only one may prevent optimizations for the other.
pub enum SetRelation {
    /// No known relationship.
    Trivial,
    /// `A` is contained entirely within `B`.
    Subset,
    /// `A` entirely contains `B`.
    Superset,
    /// `A` and `B` share no elements (Intersection is Empty).
    Disjoint,
    /// `A` and `B` cover the entire universe (Union is Universal).
    Cover,
    /// `A` is the exact inverse of `B`.
    Complementary,
    /// `A` and `B` contain exactly the same elements.
    Equal,
}

impl From<SetRelation> for MergeRelation {
    fn from(r: SetRelation) -> Self {
        match r {
            SetRelation::Trivial => MergeRelation::TRIVIAL,
            SetRelation::Subset => MergeRelation::SUBSET,
            SetRelation::Superset => MergeRelation::SUPERSET,
            SetRelation::Disjoint => MergeRelation::DISJOINT,
            SetRelation::Cover => MergeRelation::COVER,
            SetRelation::Complementary => MergeRelation::COMPLEMENTARY,
            SetRelation::Equal => MergeRelation::EQUAL,
        }
    }
}

/// The outcome of a custom merge operation.
pub enum MergeResult<T> {
    /// The merge resulted in an empty set.
    Empty,
    /// The merge resulted in a universal set.
    Universal,
    /// The merge resulted in a new set `T`.
    ///
    /// The boolean flag indicates negation:
    /// * `false`: The result is `Set`.
    /// * `true`: The result is `NOT Set`.
    Set(T, bool),
}

impl<T> From<T> for MergeResult<T> {
    fn from(value: T) -> Self {
        MergeResult::Set(value, false)
    }
}

/// A trait for injecting domain-specific logic into the optimizer.
///
/// Implementing this allows the [`Expression::optimize`](crate::expr::Expression::optimize)
/// function to understand relationships between your specific data types, enabling simplifications
/// that pure boolean logic cannot see.
///
/// # Rules
/// * **Consistency:** Logic defined here must be permanent. Do not optimize based on transient
///   data (like "Current Time").
/// * **Partial Implementation:** You do not need to handle every case. Returning
///   [`SetRelation::Trivial`] or `None` is always safe; it just means less optimization.
///
/// # Example: Role-Based Permissions
///
/// Imagine a system where the `Admin` role automatically inherits everything the `User` role has.
///
/// ```rust
/// use logify::opt::{Mergeable, SetRelation};
///
/// #[derive(PartialEq, Hash)]
/// enum Role { User, Admin, Guest }
///
/// // 1. Define a custom merger struct
/// struct RoleMerger;
///
/// // 2. Implement the trait for your struct
/// impl Mergeable<Role> for RoleMerger {
///     fn get_relation(&mut self, a: &Role, b: &Role) -> SetRelation {
///         match (a, b) {
///             // "Admin implies User" means every Admin is also a User.
///             // Therefore, the set of Admins is a SUBSET of the set of Users.
///             (Role::Admin, Role::User) => SetRelation::Subset,
///             (Role::User, Role::Admin) => SetRelation::Superset,
///             
///             // Guests and Admins don't share roles in this example
///             (Role::Guest, Role::Admin) => SetRelation::Disjoint,
///             
///             // Other cases have no relation.
///             _ => SetRelation::Trivial,
///         }
///     }
/// }
/// ```
pub trait Mergeable<T> {
    /// Determines the relationship between two sets.
    ///
    /// # Recommendations
    /// * If `a == b`, return [`SetRelation::Equal`].
    /// * If `a` implies `b`, return [`SetRelation::Subset`].
    /// * If `b` implies `a`, return [`SetRelation::Superset`].
    fn get_relation(&mut self, _a: &T, _b: &T) -> SetRelation {
        SetRelation::Trivial
    }

    /// Attempts to combine two sets using a Union (OR) operation.
    ///
    /// Return `Some` if the sets can be merged into a single node (or constant).
    ///
    /// * `a_neg`/`b_neg`: True if the set being passed in is effectively `NOT Set`.
    ///
    /// # Example
    /// * Interval merging: `[0, 5]` OR `[5, 10]` becomes `[0, 10]`.
    fn merge_union(
        &mut self,
        _a: &T,
        _a_neg: bool,
        _b: &T,
        _b_neg: bool,
    ) -> Option<MergeResult<T>> {
        None
    }

    /// Attempts to combine two sets using an Intersection (AND) operation.
    ///
    /// Return `Some` if the sets can be merged into a single node (or constant).
    ///
    /// # Example
    /// * Interval filtering: `[0, 10]` AND `[5, 15]` becomes `[5, 10]`.
    fn merge_intersection(
        &mut self,
        _a: &T,
        _a_neg: bool,
        _b: &T,
        _b_neg: bool,
    ) -> Option<MergeResult<T>> {
        None
    }
}

impl<T> Mergeable<T> for () {}

pub(crate) struct Merger<'a, T, M: Mergeable<T>> {
    pub mergeable: &'a mut M,
    cache: HashMap<(usize, usize), (MergeRelation, usize)>,
    _mergeable_type: PhantomData<T>,
}

impl<'a, T, M: Mergeable<T>> Merger<'a, T, M> {
    pub(crate) fn new(mergeable: &'a mut M) -> Self {
        Self {
            mergeable,
            cache: HashMap::new(),
            _mergeable_type: PhantomData,
        }
    }

    pub(crate) fn get_relation(
        &mut self,
        expr: &Expression<T>,
        a: NodeId,
        b: NodeId,
        depth: usize,
    ) -> MergeRelation {
        // quick returns that don't require self.mergeable.get_relation()
        if a == b {
            return MergeRelation::EQUAL;
        }
        if a == b.not() {
            return MergeRelation::COMPLEMENTARY;
        }

        self.get_relation_recursive(expr, a, b, depth)
    }

    fn get_relation_recursive(
        &mut self,
        expr: &Expression<T>,
        a: NodeId,
        b: NodeId,
        depth: usize,
    ) -> MergeRelation
    where
        M: Mergeable<T>,
    {
        // quick checks (all cases for same positives)
        if a == b {
            return MergeRelation::EQUAL;
        }
        if a == b.not() {
            return MergeRelation::COMPLEMENTARY;
        }

        // check cache
        let (min, max) = if a.idx() <= b.idx() { (a, b) } else { (b, a) };
        let key = (min.idx(), max.idx());

        // if the realationship is already cached at a depth greater than or equal to own, return that relationship
        if let Some(&(cached_rel, cached_depth)) = self.cache.get(&key)
            && cached_depth >= depth
        {
            // Determine if we need to flip the result based on input order
            let mut final_rel = cached_rel;
            if a != min {
                final_rel = final_rel.flip();
            }
            return self.apply_negation_logic(final_rel, a.is_neg(), b.is_neg());
        }

        // don't cache, a higher depth will always replace it
        if depth == 0 {
            return MergeRelation::TRIVIAL;
        }

        let node_min = &expr.nodes[min.idx()];
        let node_max = &expr.nodes[max.idx()];

        let rel = match (node_min, node_max) {
            // EMPTY is equal to EMPTY
            (Node::Empty, Node::Empty) => MergeRelation::EQUAL, // handled by a==b, but just to make sure
            // EMPTY is disjoint from everything
            (Node::Empty, _) | (_, Node::Empty) => MergeRelation::DISJOINT,
            // Set and Set
            (Node::Set(set_min), Node::Set(set_max)) => {
                self.mergeable.get_relation(set_min, set_max).into()
            }
            // Set and Group
            (Node::Set(_), Node::Union(kids_b)) | (Node::Set(_), Node::Intersection(kids_b)) => {
                let is_union = matches!(node_max, Node::Union(_));
                self.get_groups_relation(expr, &[min], is_union, kids_b, is_union, depth - 1)
            }
            // Group and Set
            (Node::Union(kids_a), Node::Set(_)) | (Node::Intersection(kids_a), Node::Set(_)) => {
                let is_union = matches!(node_min, Node::Union(_));
                self.get_groups_relation(expr, kids_a, is_union, &[max], is_union, depth - 1)
            }
            // Group and Group
            (Node::Union(kids_a), Node::Union(kids_b))
            | (Node::Union(kids_a), Node::Intersection(kids_b))
            | (Node::Intersection(kids_a), Node::Union(kids_b))
            | (Node::Intersection(kids_a), Node::Intersection(kids_b)) => self.get_groups_relation(
                expr,
                kids_a,
                matches!(node_min, Node::Union(_)),
                kids_b,
                matches!(node_max, Node::Union(_)),
                depth - 1,
            ),
        };

        // equal and complementary can't be improved
        let stored_depth = if rel == MergeRelation::EQUAL || rel == MergeRelation::COMPLEMENTARY {
            usize::MAX
        } else {
            depth
        };
        self.cache.insert(key, (rel, stored_depth)); // push to cache

        // return the relationship
        let mut final_rel = rel;
        if a != min {
            final_rel = final_rel.flip();
        }
        self.apply_negation_logic(final_rel, a.is_neg(), b.is_neg())
    }

    fn apply_negation_logic(&self, rel: MergeRelation, neg_a: bool, neg_b: bool) -> MergeRelation {
        if !neg_a && !neg_b {
            return rel;
        }

        // start with trivial relationship
        let mut result = MergeRelation::TRIVIAL;

        // A == B
        if rel == MergeRelation::EQUAL {
            return if neg_a == neg_b {
                // A' == B'
                MergeRelation::EQUAL
            } else {
                // A' comp B, A comp B'
                MergeRelation::COMPLEMENTARY
            };
        }

        // A comp B
        if rel == MergeRelation::COMPLEMENTARY {
            return if neg_a == neg_b {
                // A' comp B'
                MergeRelation::COMPLEMENTARY
            } else {
                // A' == B, B' == A
                MergeRelation::EQUAL
            };
        }

        // A sub B
        if rel.is_subset() {
            match (neg_a, neg_b) {
                (true, true) => result |= MergeRelation::SUPERSET, // A' sup B'
                (false, true) => result |= MergeRelation::DISJOINT, // A disj B'
                _ => {}
            }
        }

        // A sup B
        if rel.is_superset() {
            match (neg_a, neg_b) {
                (true, true) => result |= MergeRelation::SUBSET, // A' sub B'
                (true, false) => result |= MergeRelation::DISJOINT, // A' disj B
                _ => {}
            }
        }

        // A disj B
        if rel.is_disjoint() {
            match (neg_a, neg_b) {
                (false, true) => result |= MergeRelation::SUBSET, // A sub B'
                (true, false) => result |= MergeRelation::SUPERSET, // A' sup B
                _ => {}
            }
        }

        // A | B = U
        if rel.is_cover() {
            match (neg_a, neg_b) {
                (false, true) => result |= MergeRelation::SUPERSET, // A sup B'
                (true, false) => result |= MergeRelation::SUBSET,   // A' sub B
                _ => {}
            }
        }

        // return modified result
        result
    }

    fn get_groups_relation(
        &mut self,
        expr: &Expression<T>,
        kids_a: &[NodeId],
        is_union_a: bool,
        kids_b: &[NodeId],
        is_union_b: bool,
        depth: usize,
    ) -> MergeRelation
    where
        M: Mergeable<T>,
    {
        // cover test omitted, should be covered with merging

        // begin with trivial relationship
        let mut result = MergeRelation::TRIVIAL;

        // check disjoint
        let is_disjoint = match (is_union_a, is_union_b) {
            // Intersection A, Intersection B, best O(1)
            (false, false) =>
            // any a disjoint from any b
            {
                kids_a.iter().any(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_disjoint())
                })
            }
            // Union A, Intersection B, best O(A)
            (true, false) =>
            // all a disjoint from any b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_disjoint())
                })
            }
            // Intersection A, Union B, best O(B)
            (false, true) =>
            // all b dijoint from any a
            {
                kids_b.iter().all(|&b| {
                    kids_a
                        .iter()
                        .any(|&a| self.get_relation_recursive(expr, a, b, depth).is_disjoint())
                })
            }
            // Union A, Union B, best O(A*B)
            (true, true) =>
            // all a disjoint from all b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .all(|&b| self.get_relation_recursive(expr, a, b, depth).is_disjoint())
                })
            }
        };
        if is_disjoint {
            result |= MergeRelation::DISJOINT;
        }

        // check subset
        let is_subset = match (is_union_a, is_union_b) {
            (true, true) =>
            // UU, best O(A)
            // all a subset any b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_subset())
                })
            }
            (true, false) =>
            // UI, best O(AB)
            // all a subset all b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .all(|&b| self.get_relation_recursive(expr, a, b, depth).is_subset())
                })
            }
            (false, true) =>
            // IU, best O(1)
            // any a subset any b
            {
                kids_a.iter().any(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_subset())
                })
            }
            (false, false) =>
            // II, best O(B)
            // all b superset any a
            {
                kids_b.iter().all(|&b| {
                    kids_a
                        .iter()
                        .any(|&a| self.get_relation_recursive(expr, a, b, depth).is_superset())
                })
            }
        };
        if is_subset {
            result |= MergeRelation::SUBSET;
        }

        // check superset
        let is_superset = match (is_union_a, is_union_b) {
            (true, true) =>
            // UU, best O(B)
            // all b subset any a
            {
                kids_b.iter().all(|&b| {
                    kids_a
                        .iter()
                        .any(|&a| self.get_relation_recursive(expr, a, b, depth).is_subset())
                })
            }
            (true, false) =>
            // UI, best O(1)
            // any a superset any b
            {
                kids_a.iter().any(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_superset())
                })
            }
            (false, true) =>
            // IU, best O(AB)
            // all a superset all b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .all(|&b| self.get_relation_recursive(expr, a, b, depth).is_superset())
                })
            }
            (false, false) =>
            // II, best O(A)
            // all a superset any b
            {
                kids_a.iter().all(|&a| {
                    kids_b
                        .iter()
                        .any(|&b| self.get_relation_recursive(expr, a, b, depth).is_superset())
                })
            }
        };
        if is_superset {
            result |= MergeRelation::SUPERSET;
        }

        // return modified result
        result
    }
}
