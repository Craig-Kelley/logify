use std::hash::Hash;

use slotmap::{SecondaryMap, SlotMap};

use crate::{
    builder::{BuilderNode, ExpressionBuilder, NodeHandle},
    expr::{Expression, NodeId},
};

impl<T: Hash + PartialEq> ExpressionBuilder<T> {
    /// Compiles the builder into an optimized `Expression`.
    ///
    /// This consumes the builder.
    ///
    /// # Optimization Steps
    /// 1. **Deduplication:** Identical logic branches are merged.
    /// 2. **Pruning:** Nodes not connected to an added root are removed.
    /// 3. **Resolution:** Pointers to deleted nodes are resolved to Empty.
    /// 4. **Cycle Removal:** Recursive loops are detected and broken.
    pub fn build(self) -> Expression<T> {
        let mut expr = Expression::new();
        expr.extend(self);
        expr
    }

    /// Compiles the builder and merges it into an existing `Expression`.
    ///
    /// This allows you to append new roots to an existing structure without
    /// rebuilding the entire graph.
    pub fn build_into(self, mut expr: Expression<T>) {
        expr.extend(self);
    }
}

impl<T> IntoIterator for ExpressionBuilder<T> {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

impl<T: Hash + PartialEq> Extend<ExpressionBuilder<T>> for Expression<T> {
    fn extend<I: IntoIterator<Item = ExpressionBuilder<T>>>(&mut self, iter: I) {
        for source in iter {
            let nodes = source.nodes.into_inner();
            if nodes.is_empty() {
                continue;
            }
            let roots = source.roots.into_inner();
            ExpressionBuilder::stack_into(self, nodes, &roots);
        }
    }
}

impl<T> IntoIterator for &ExpressionBuilder<T> {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

impl<'a, T: Hash + PartialEq + Clone> Extend<&'a ExpressionBuilder<T>> for Expression<T> {
    fn extend<I: IntoIterator<Item = &'a ExpressionBuilder<T>>>(&mut self, iter: I) {
        for builder in iter {
            let nodes = builder.nodes.borrow().clone();
            let roots = builder.roots.borrow();
            ExpressionBuilder::stack_into(self, nodes, &roots);
        }
    }
}

impl<T: Hash + PartialEq> ExpressionBuilder<T> {
    fn stack_into(
        expr: &mut Expression<T>,
        mut nodes: SlotMap<NodeHandle, BuilderNode<T>>,
        roots: &[NodeHandle],
    ) {
        let mut map = SecondaryMap::new();
        // tracks nodes on the stack, preventing loops
        let mut on_stack = SecondaryMap::new();
        let mut stack = Vec::new();

        for &root in roots {
            // check if already processed
            if let Some(&cached) = map.get(root) {
                expr.add_root(cached);
                continue;
            }

            stack.clear();
            stack.push((root, false));
            on_stack.insert(root, ());
            while let Some((handle, visited)) = stack.pop() {
                // skip already processed nodes
                if map.contains_key(handle) {
                    on_stack.remove(handle);
                    continue;
                }

                if visited {
                    // already processed all children, can now process this
                    on_stack.remove(handle);
                    let node = nodes.remove(handle).unwrap_or(BuilderNode::Empty);

                    let dest_id = match node {
                        BuilderNode::Empty => NodeId::EMPTY,
                        BuilderNode::Universal => NodeId::UNIVERSAL,
                        BuilderNode::Set(value) => expr.set(value),
                        BuilderNode::Not(child) => {
                            let child_id = map.get(child).copied().unwrap_or(NodeId::EMPTY);
                            expr.complement(child_id)
                        }
                        BuilderNode::Union(kids) => {
                            let mapped_kids = kids
                                .iter()
                                .map(|k| map.get(*k).copied().unwrap_or(NodeId::EMPTY));
                            expr.union(mapped_kids)
                        }
                        BuilderNode::Intersection(kids) => {
                            let mapped_kids = kids
                                .iter()
                                .map(|k| map.get(*k).copied().unwrap_or(NodeId::EMPTY));
                            expr.intersection(mapped_kids)
                        }
                    };
                    map.insert(handle, dest_id);
                } else {
                    // kids to push
                    let kids_to_visit = match nodes.get(handle) {
                        Some(BuilderNode::Union(kids)) | Some(BuilderNode::Intersection(kids)) => {
                            Some(kids.clone())
                        }
                        Some(BuilderNode::Not(kid)) => Some(vec![*kid]),
                        _ => None,
                    };

                    stack.push((handle, true));

                    if let Some(kids) = kids_to_visit {
                        for &k in kids.iter().rev() {
                            if map.contains_key(k) {
                                continue;
                            }
                            if on_stack.contains_key(k) {
                                continue;
                            }
                            on_stack.insert(k, ());
                            stack.push((k, false));
                        }
                    }
                }
            }

            // add the root
            let final_root = map.get(root).copied().unwrap_or(NodeId::EMPTY);
            expr.add_root(final_root);
        }
    }
}
