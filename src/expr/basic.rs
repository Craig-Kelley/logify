use std::{
    fmt::Display,
    hash::{BuildHasher, Hash},
    slice::Iter,
};

use hashbrown::hash_map::RawEntryMut;

use crate::expr::{Expression, Node, NodeId, iter::ExpressionDependencyIter};

impl<T> Expression<T> {
    /// Creates a new, empty Expression.
    pub fn new() -> Self {
        Self::default()
    }

    /// registers a node as a "Root" of the expression.
    ///
    /// Roots are the entry points for evaluation and dependency iteration.
    /// Nodes not reachable from a root are considered dead code.
    ///
    /// # Panics
    /// Panics if `root` is not a valid ID belonging to this expression.
    ///
    /// # Example
    /// ```rust
    /// let mut expr = logify::Expression::new();
    /// let a = expr.set("A");
    /// expr.add_root(a);
    /// ```
    pub fn add_root(&mut self, root: NodeId) {
        if root.idx() >= self.nodes.len() {
            panic!(
                "Invalid NodeId: ID {} for node {} does not exist in this expression. The expression has {} nodes.",
                root.raw(),
                root.idx(),
                self.nodes.len(),
            );
        }
        self.roots.push(root);
    }

    /// A helper to build logic and add it as a root in one closure.
    ///
    /// This pattern is often more ergonomic than manually creating variables
    /// and passing them to `add_root`.
    ///
    /// # Example
    /// ```rust
    /// let mut expr = logify::Expression::new();
    ///
    /// // Build (A & B) and add it as a root immediately
    /// expr.build_root(|e| {
    ///     let a = e.set("A");
    ///     let b = e.set("B");
    ///     e.intersection([a, b])
    /// });
    /// ```
    pub fn build_root(&mut self, root: impl FnOnce(&mut Self) -> NodeId) {
        let root = root(self);
        self.add_root(root);
    }

    /// Iterate over the registered root IDs.
    pub fn roots(&self) -> Iter<'_, NodeId> {
        self.roots.iter()
    }

    /// Returns the number of roots.
    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    /// Iterate linearly over the raw internal nodes.
    ///
    /// *Note: This iterates the storage vector directly. It includes dead nodes
    /// and does not respect topological order.*
    pub fn nodes(&self) -> Iter<'_, Node<T>> {
        self.nodes.iter()
    }

    /// Returns the total number of nodes (active and dead) in memory.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns an iterator that visits nodes in topological order.
    ///
    /// This is useful for evaluation or compilation, as it guarantees that
    /// a node's dependencies (children) are yielded before the node itself.
    ///
    /// * **Post-Order:** Children before Parents.
    /// * **Pruned:** Only visits nodes reachable from the roots.
    /// * **Unique:** Visits each reachable node exactly once.
    pub fn iter_dependencies(&self) -> ExpressionDependencyIter<'_, T> {
        ExpressionDependencyIter::new(self)
    }
}

impl<T: Hash + PartialEq> Expression<T> {
    pub(crate) fn alloc(&mut self, node: Node<T>) -> NodeId {
        if let Node::Empty = node {
            return NodeId::EMPTY;
        }

        let hasher_builder = *self.cache.hasher();
        let hash = hasher_builder.hash_one(&node);

        let nodes = &self.nodes;
        let entry = self
            .cache
            .raw_entry_mut()
            .from_hash(hash, |&id| nodes[id.idx()] == node);
        match entry {
            RawEntryMut::Occupied(entry) => *entry.key(), // duplicate node
            RawEntryMut::Vacant(entry) => {
                // save the new node
                let id = NodeId::new(self.nodes.len() as u32, false);
                self.nodes.push(node);

                // add the entry hash for later duplicate detection
                entry.insert_with_hasher(hash, id, (), |&id| {
                    // let mut hasher = hasher_builder.build_hasher();
                    // self.nodes[id.idx()].hash(&mut hasher);
                    // hasher.finish()

                    hasher_builder.hash_one(&self.nodes[id.idx()])
                });
                id
            }
        }
    }

    /// Creates a leaf node representing a specific value `A`.
    ///
    /// If an identical set `A` already exists, the existing ID is returned.
    ///
    /// # Example
    /// ```rust
    /// let mut expr = logify::Expression::new();
    /// let a1 = expr.set("TagA");
    /// let a2 = expr.set("TagA");
    ///
    /// assert_eq!(a1, a2); // Deduplication happens automatically
    /// ```
    pub fn set(&mut self, value: T) -> NodeId {
        self.alloc(Node::Set(value))
    }

    /// Creates a logical Union (`A OR B`).
    ///
    /// This method acts as a **Smart Constructor**. It performs immediate on-the-fly
    /// simplifications to keep the graph small.
    ///
    /// # Simplifications Performed
    /// * **Commutativity:** `B | A` -> `A | B` (sorted).
    /// * **Idempotence:** `A | A` -> `A`.
    /// * **Identity:** `A | Empty` -> `A`.
    /// * **Annihilation:** `A | Universal` -> `Universal`.
    /// * **Complements:** `A | !A` -> `Universal`.
    /// * **Singleton:** `Union([A])` -> `A`.
    ///
    /// # Example
    /// ```rust
    /// # use logify::Expression;
    /// let mut expr = Expression::new();
    /// let a = expr.set("A");
    /// let b = expr.set("B");
    ///
    /// // Standard Union
    /// let a_or_b = expr.union([a, b]);
    ///
    /// // Simplification: A | A == A
    /// let a_or_a = expr.union([a, a]);
    /// assert_eq!(a_or_a, a);
    /// ```
    pub fn union(&mut self, children: impl IntoIterator<Item = NodeId>) -> NodeId {
        let mut children: Vec<NodeId> = children.into_iter().collect();

        // places A and !A next to each other
        children.sort_unstable(); // commutative, B | A == A | B
        children.dedup(); // idempotent, A | A == A

        // identity and annulment
        // remove Empty (E | A == A) and test for Universal (U | A == U)
        if let Some(&first) = children.first() {
            if first == NodeId::UNIVERSAL {
                return NodeId::UNIVERSAL;
            }
            if first == NodeId::EMPTY {
                if children.get(1) == Some(&NodeId::UNIVERSAL) {
                    return NodeId::UNIVERSAL;
                }
                children.remove(0); // TODO: O(N) SHIFT!!
            }
        }

        // universality, A | !A == U
        for w in children.windows(2) {
            if w[0].idx() == w[1].idx() {
                return NodeId::UNIVERSAL;
            }
        }

        // simplify
        if children.is_empty() {
            return NodeId::EMPTY; // Union(_) == E
        }
        if children.len() == 1 {
            return children[0]; // Union(A) == A
        }
        self.alloc(Node::Union(children))
    }

    /// Creates a logical Intersection (`A AND B`).
    ///
    /// This method acts as a **Smart Constructor**. It performs immediate on-the-fly
    /// simplifications to keep the graph small.
    ///
    /// # Simplifications Performed
    /// * **Commutativity:** `B & A` -> `A & B` (sorted).
    /// * **Idempotence:** `A & A` -> `A`.
    /// * **Identity:** `A & Universal` -> `A`.
    /// * **Annihilation:** `A & Empty` -> `Empty`.
    /// * **Complements:** `A & !A` -> `Empty`.
    /// * **Singleton:** `Intersection([A])` -> `A`.
    ///
    /// # Example
    /// ```rust
    /// # use logify::Expression;
    /// let mut expr = Expression::new();
    /// let a = expr.set("A");
    /// let not_a = expr.complement(a);
    ///
    /// // Simplification: A & !A == Empty
    /// let impossible = expr.intersection([a, not_a]);
    /// assert_eq!(impossible, logify::NodeId::EMPTY);
    /// ```
    pub fn intersection(&mut self, children: impl IntoIterator<Item = NodeId>) -> NodeId {
        let mut children: Vec<NodeId> = children.into_iter().collect();

        // places A and !A next to each other
        children.sort_unstable(); // commutative, B & A == A & B
        children.dedup(); // idempotent, A & A == A

        // identity and annulment
        // remove Universal (U & A == A) and test for Empty (E & A == E)
        if let Some(&first) = children.first() {
            if first == NodeId::EMPTY {
                return NodeId::EMPTY;
            }
            if first == NodeId::UNIVERSAL {
                children.remove(0);
            }
        }

        // annihilation, A & !A == E
        for w in children.windows(2) {
            if w[0].idx() == w[1].idx() {
                return NodeId::EMPTY;
            }
        }

        // simplify
        if children.is_empty() {
            return NodeId::UNIVERSAL; // Intersection(_) == U
        }
        if children.len() == 1 {
            return children[0]; // Intersection(A) == A
        }
        self.alloc(Node::Intersection(children))
    }

    /// Returns the complement A => A'.
    pub fn complement(&self, child: NodeId) -> NodeId {
        child.not()
    }
}

impl<T: Display> Expression<T> {
    /// Recursively formats the expression starting from the given root.
    ///
    /// # Example
    /// ```rust
    /// # use logify::Expression;
    /// let mut expr = Expression::new();
    /// let a = expr.set("A");
    /// let b = expr.set("B");
    /// let root = expr.intersection([a, b]);
    ///
    /// assert_eq!(expr.to_string(&root), "([A] & [B])");
    /// ```
    pub fn to_string(&self, root: &NodeId) -> String {
        let is_neg = if root.is_neg() { "'" } else { "" };
        match &self.nodes[root.idx()] {
            Node::Set(set) => format!("[{}]{}", set, is_neg,),
            Node::Union(children) => {
                let sets: Vec<_> = children.iter().map(|&id| self.to_string(&id)).collect();
                format!("({}){}", sets.join(" | "), is_neg,)
            }
            Node::Intersection(children) => {
                let sets: Vec<_> = children.iter().map(|&id| self.to_string(&id)).collect();
                format!("({}){}", sets.join(" & "), is_neg,)
            }
            Node::Empty => {
                if root.is_neg() {
                    "UNIVERSAL".to_string()
                } else {
                    "EMPTY".to_string()
                }
            }
        }
    }
}
