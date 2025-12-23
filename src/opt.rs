use std::hash::Hash;

use crate::{
    expr::{Expression, Node, NodeId},
    opt::merger::Merger,
};

mod algo;
mod merger;

pub use merger::{MergeResult, Mergeable, SetRelation};

/// Configuration for the [`Expression::optimize`] method.
///
/// This struct controls how aggressively the optimizer searches for simplifications.
/// Higher depths and iteration limits can produce smaller expressions but take longer to run.
pub struct OptimizerConfig<M> {
    /// The user-defined merger logic.
    ///
    /// This allows domain-specific logic (e.g., knowing that "Tag A" implies "Tag B")
    /// to be injected into the optimization process.
    pub merger: M,

    /// The recursion depth for relationship lookups.
    ///
    /// Defines how deep the merger looks when comparing nodes to find simplifications.
    ///
    /// # Levels
    /// * **`0` (Syntax Only):** Disables `get_relation`. Only performs structural simplifications
    ///   like flattening nested unions and applying De Morgan's laws.
    /// * **`1` (Shallow):** Checks relationships between immediate Sets.
    ///   * Finds: `A` vs `B` (via `Mergeable`).
    ///   * Misses: `A` vs `(A | B)`.
    /// * **`2` (Standard):** Peeks inside one layer of Groups.
    ///   * Finds: `A` vs `(A | B)` (Absorption).
    /// * **`N > 2` (Deep):** Recursively scans children of children.
    ///   * Finds: `A` vs `(B | (C | A))`.
    ///   * *Warning:* High depths can be expensive for deeply nested expressions.
    pub merger_depth: usize,

    /// The maximum number of optimization passes.
    ///
    /// The optimizer runs in a loop, feeding the output of one pass into the next,
    /// until the expression stops changing (reaches a fixed point).
    ///
    /// # Values
    /// * **`0` (Default):** Run until the expression stabilizes (no more changes occur).
    /// * **`N > 0`:** Run at most `N` passes.
    ///
    /// Limiting iterations is rarely necessary as the optimizer converges quickly,
    /// but it can be used to guarantee a strict time budget.
    pub max_iterations: usize,
}

impl Default for OptimizerConfig<()> {
    fn default() -> Self {
        Self {
            merger: (),
            merger_depth: 2,
            max_iterations: 0,
        }
    }
}

impl<T: Hash + PartialEq> Expression<T> {
    /// Applies logic reduction and domain-specific simplification to the expression.
    ///
    /// This method performs operations such as:
    /// * **Flattening:** `Union(A, Union(B, C))` becomes `Union(A, B, C)`.
    /// * **De Morgan's Laws:** Distributes negations to minimize depth.
    /// * **Absorption:** `A & (A | B)` simplifies to `A`.
    /// * **Custom Merging:** Uses the provided [`Mergeable`] implementation to combine sets.
    ///
    /// # Dead Nodes
    /// Optimization rewrites connections between nodes. This often leaves behind "dead" nodes
    /// (nodes that are no longer connected to any root). While this does not affect evaluation
    /// correctness, you may wish to call [`Expression::clean`](crate::Expression::clean) afterwards
    /// if memory footprint is a concern.
    pub fn optimize<M: Mergeable<T>>(&mut self, config: &mut OptimizerConfig<M>) {
        // merger initialization
        let mut merger = Merger::new(&mut config.merger);

        // maps old nodes to newer optimized ones
        let mut remap = vec![NodeId::MAX; self.nodes.len()];

        // loop through until there's no more nodes to optimize
        let mut i = 0;
        let mut iter_count = 0;
        let mut iter_end = self.nodes.len();
        while i < self.nodes.len() {
            // optimize the node, possibly creating a new node id
            let new_id = match &self.nodes[i] {
                Node::Empty => NodeId::EMPTY,
                Node::Set(_) => NodeId::new(i as u32, false),
                Node::Union(kids) => {
                    let kids = kids.iter().map(|&k| resolve(k, &remap)).collect();
                    self.apply_logic_reduction(kids, true, &mut merger, config.merger_depth)
                }
                Node::Intersection(kids) => {
                    let kids = kids.iter().map(|&k| resolve(k, &remap)).collect();
                    self.apply_logic_reduction(kids, false, &mut merger, config.merger_depth)
                }
            };

            // update the remap for this node
            if new_id.idx() < i {
                // if the new_id is a previous node, take the previous node's optimized form
                remap[i] = resolve(new_id, &remap);
            } else {
                // if the new_id is not a previous node, this new_id is the optimized form
                remap[i] = new_id;
            }

            // max iterations
            i += 1;
            if i >= iter_end {
                if config.max_iterations != 0 {
                    iter_count += 1;
                    if iter_count >= config.max_iterations {
                        break;
                    }
                }
                // resize remap for new nodes
                iter_end = self.nodes.len();
                remap.resize(iter_end, NodeId::MAX);
            }
        }

        // remap roots
        for root in &mut self.roots {
            *root = resolve(*root, &remap);
        }
    }
}

// for mapping to a node that is already processed, while respecting sign
fn resolve(mut id: NodeId, remap: &[NodeId]) -> NodeId {
    loop {
        let idx = id.idx();
        if idx >= remap.len() || remap[idx] == NodeId::MAX {
            return id; // not processed
        }

        // get optimized node
        let opt = remap[idx];
        if opt.idx() == idx {
            return id; // return the id once it matches the optimized id
        }

        // id is now the optimized one
        if id.is_neg() {
            id = opt.not();
        } else {
            id = opt;
        }
    }
}
