use crate::expr::{Expression, Node, NodeId};

/// An iterator that visits nodes in topological (Post-Order) order.
///
/// This iterator traverses the graph starting from the roots, ensuring that **children are yielded
/// before their parents**. This is the optimal order for tasks like:
/// * **Evaluation:** You can compute child results before processing the parent.
/// * **Compilation:** You can generate code for sub-expressions before the main expression.
/// * **Serialization:** You can serialize dependencies first.
///
/// # Behavior
/// * **Iterative:** Uses an explicit stack, so it is safe for very deep graphs (no stack overflow).
/// * **Deduplicated:** Shared nodes (diamonds in the graph) are yielded exactly once.
/// * **Pruned:** Only nodes reachable from the `Expression`'s roots are visited.
pub struct ExpressionDependencyIter<'a, T> {
    expr: &'a Expression<T>,
    stack: Vec<(NodeId, bool)>,
    visited: Vec<bool>, // TODO: would a bitset be faster?
}

impl<'a, T> ExpressionDependencyIter<'a, T> {
    pub(crate) fn new(expr: &'a Expression<T>) -> Self {
        let stack = expr.roots.iter().map(|&id| (id, false)).collect();
        Self {
            expr,
            stack,
            visited: vec![false; expr.nodes.len()],
        }
    }
}

impl<'a, T> Iterator for ExpressionDependencyIter<'a, T> {
    type Item = (NodeId, &'a Node<T>);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((id, expanded)) = self.stack.pop() {
            if self.visited[id.idx()] {
                continue;
            }
            if expanded {
                self.visited[id.idx()] = true;
                return Some((id, &self.expr.nodes[id.idx()]));
            } else {
                // mark self as expanded, visit children first
                self.stack.push((id, true));
                match &self.expr.nodes[id.idx()] {
                    Node::Union(kids) | Node::Intersection(kids) => {
                        for &k in kids.iter().rev() {
                            if !self.visited[k.idx()] {
                                self.stack.push((k, false));
                            }
                        }
                    }
                    _ => {} // no children
                }
            }
        }
        None
    }
}
