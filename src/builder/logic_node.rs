use std::ops::{BitAnd, BitOr, Not};

use crate::builder::{ExpressionBuilder, NodeHandle};

#[derive(Clone, Copy)]
pub struct LogicNode<'a, T> {
    builder: &'a ExpressionBuilder<T>,
    handle: NodeHandle,
}

impl<'a, T> LogicNode<'a, T> {
    pub fn new(builder: &'a ExpressionBuilder<T>, handle: NodeHandle) -> Self {
        Self { builder, handle }
    }

    pub fn handle(&self) -> NodeHandle {
        self.handle
    }
}

impl<'a, T> From<LogicNode<'a, T>> for NodeHandle {
    fn from(node: LogicNode<'a, T>) -> Self {
        node.handle
    }
}

impl<'a, T> From<&LogicNode<'a, T>> for NodeHandle {
    fn from(node: &LogicNode<'a, T>) -> Self {
        node.handle
    }
}

impl<T> ExpressionBuilder<T> {
    /// Creates a `LogicNode` wrapper around a value.
    ///
    /// This creates a node in the builder and returns a helper struct that supports
    /// operator overloading (`&`, `|`, `!`) for more ergonomic construction.
    ///
    /// # Example
    /// ```rust
    /// use logify::ExpressionBuilder;
    ///
    /// let builder = ExpressionBuilder::<&str>::new();
    /// let a = builder.leaf("A");
    /// let b = builder.leaf("B");
    ///
    /// // LogicNode supports standard operators
    /// let c = a & !b;
    /// ```
    pub fn leaf(&self, val: impl Into<T>) -> LogicNode<'_, T> {
        let h = self.set(val.into());
        LogicNode::new(self, h)
    }

    /// Wraps an existing handle in a `LogicNode` helper.
    ///
    /// Useful if you have a raw `NodeHandle` (perhaps from `builder.union(...)`)
    /// but want to switch back to using operator overloading.
    pub fn wrap(&self, handle: NodeHandle) -> LogicNode<'_, T> {
        LogicNode::new(self, handle)
    }
}

impl<'a, T> BitOr for LogicNode<'a, T> {
    type Output = LogicNode<'a, T>;

    fn bitor(self, rhs: Self) -> Self::Output {
        let new_handle = self.builder.union(vec![self.handle, rhs.handle]);
        LogicNode {
            builder: self.builder,
            handle: new_handle,
        }
    }
}

impl<'a, T> BitAnd for LogicNode<'a, T> {
    type Output = LogicNode<'a, T>;
    fn bitand(self, rhs: Self) -> Self::Output {
        let new_handle = self.builder.intersection(vec![self.handle, rhs.handle]);
        LogicNode {
            builder: self.builder,
            handle: new_handle,
        }
    }
}

impl<'a, T> Not for LogicNode<'a, T> {
    type Output = LogicNode<'a, T>;

    fn not(self) -> Self::Output {
        let new_handle = self.builder.not(self.handle);
        LogicNode {
            builder: self.builder,
            handle: new_handle,
        }
    }
}
