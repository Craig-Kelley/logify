use std::{cell::RefCell, hash::Hash};

use slotmap::{SlotMap, new_key_type};

mod convert;
mod logic_node;

new_key_type! {
    /// A lightweight handle to a node within an [`ExpressionBuilder`].
    ///
    /// `NodeHandle`s serve as opaque keys that allow you to reference and connect
    /// nodes during the construction of a logical expression.
    ///
    /// * **Copyable:** Handles are small (`u64` equivalent) and cheap to copy/pass by value.
    /// * **Scoped:** A handle is only valid for the `ExpressionBuilder` that created it.
    pub struct NodeHandle;
}

/// Represents the raw structure of a node during the build phase.
///
/// While `ExpressionBuilder` manages these internally, this enum is exposed to allow
/// for inspection or manual traversal of the build graph if necessary.
#[derive(Hash, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum BuilderNode<T> {
    /// Represents the empty set.
    /// * **Union Identity:** `A | Empty == A`
    /// * **Intersection Annihilator:** `A & Empty == Empty`
    Empty,

    /// Represents the universal set.
    /// * **Union Annihilator:** `A | Universal == Universal`
    /// * **Intersection Identity:** `A & Universal == A`
    Universal,

    /// A leaf node containing a user-provided value.
    Set(T),

    /// A logical disjunction (OR).
    /// Is true if *any* of the children are true.
    Union(Vec<NodeHandle>),

    /// A logical conjunction (AND).
    /// Is true if *all* of the children are true.
    Intersection(Vec<NodeHandle>),

    /// A logical negation (NOT).
    /// Inverts the truth value of the child.
    Not(NodeHandle),
}

/// A staging area for constructing logical expressions.
///
/// The `ExpressionBuilder` allows you to create complex logical relationships incrementally.
/// Unlike [`Expression`], which is immutable and optimized, the builder allows for interior
/// mutability, flexible node connection, and arbitrary ordering.
///
/// # Logic Nodes & Macros
/// You can use the builder in three main ways depending on your preference:
/// 1. **Direct Handles:** Manually wiring `NodeHandle`s (best for programmatic generation).
/// 2. **Operator Overloading:** Using `.leaf()` to get objects that support `&`, `|`, `!`.
/// 3. **Macros:** Using `logic!`, `any!`, and `all!` for a visual representation.
///
/// # Example 1: Direct Handles (Programmatic)
/// ```rust
/// use logify::ExpressionBuilder;
///
/// let builder = ExpressionBuilder::<&str>::new();
///
/// // Create leaves: A, B, C
/// let a = builder.set("A");
/// let b = builder.set("B");
/// let c = builder.set("C");
///
/// // Logic: (A | B) & !C
/// let a_or_b = builder.union([a, b]);
/// let not_c = builder.not(c);
/// let root = builder.intersection([a_or_b, not_c]);
///
/// builder.add_root(root);
/// let expr = builder.build();
/// ```
///
/// # Example 2: Operator Style (Ergonomic)
/// ```rust
/// use logify::ExpressionBuilder;
///
/// let builder = ExpressionBuilder::<&str>::new();
///
/// // Create wrappers that support operators
/// let a = builder.leaf("A");
/// let b = builder.leaf("B");
/// let c = builder.leaf("C");
///
/// // Natural syntax: (A OR B) AND (NOT C)
/// let logic = (a | b) & !c;
///
/// builder.add_root(logic);
/// ```
///
/// # Example 3: Macro Style (Visual)
/// ```rust
/// use logify::{ExpressionBuilder, logic};
///
/// let builder = ExpressionBuilder::<&str>::new();
///
/// // Construct complex logic trees visually
/// let root = logic!(builder,
///     any![
///         "A",
///         "B",
///         all!["C", "D", "E"]
///     ]
/// );
///
/// builder.add_root(root);
/// ```
#[derive(Clone)]
pub struct ExpressionBuilder<T> {
    pub nodes: RefCell<SlotMap<NodeHandle, BuilderNode<T>>>,
    pub roots: RefCell<Vec<NodeHandle>>,
}

impl<T> Default for ExpressionBuilder<T> {
    fn default() -> Self {
        Self {
            nodes: RefCell::new(SlotMap::with_key()),
            roots: RefCell::new(Vec::new()),
        }
    }
}

impl<T> ExpressionBuilder<T> {
    /// Creates a new, empty `ExpressionBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a set (a leaf node) containing the given value.
    ///
    /// # Arguments
    /// * `val` - The value to store. Accepts any type that implements `Into<T>`.
    pub fn set(&self, val: impl Into<T>) -> NodeHandle {
        self.nodes.borrow_mut().insert(BuilderNode::Set(val.into()))
    }

    /// Creates a constant Empty set node.
    ///
    /// * **Union Identity:** `A | Empty == A`
    /// * **Intersection Annihilator:** `A & Empty == Empty`
    pub fn empty(&self) -> NodeHandle {
        self.nodes.borrow_mut().insert(BuilderNode::Empty)
    }

    /// Creates a constant Universal set node.
    ///
    /// * **Union Annihilator:** `A | Universal == Universal`
    /// * **Intersection Identity:** `A & Universal == A`
    pub fn universal(&self) -> NodeHandle {
        self.nodes.borrow_mut().insert(BuilderNode::Universal)
    }

    /// Creates a Union (OR) node from the provided children.
    ///
    /// Represents logic where *at least one* child must be true.
    ///
    /// # Arguments
    /// * `kids` - An iterator of `NodeHandle`s (or items that convert into them).
    pub fn union<H: Into<NodeHandle>>(&self, kids: impl IntoIterator<Item = H>) -> NodeHandle {
        let kids = kids.into_iter().map(|h| h.into()).collect();
        self.nodes.borrow_mut().insert(BuilderNode::Union(kids))
    }

    /// Creates an Intersection (AND) node from the provided children.
    ///
    /// Represents logic where *all* children must be true.
    ///
    /// # Arguments
    /// * `kids` - An iterator of `NodeHandle`s (or items that convert into them).
    pub fn intersection<H: Into<NodeHandle>>(
        &self,
        kids: impl IntoIterator<Item = H>,
    ) -> NodeHandle {
        let kids = kids.into_iter().map(|h| h.into()).collect();
        self.nodes
            .borrow_mut()
            .insert(BuilderNode::Intersection(kids))
    }

    /// Creates a Complement (NOT) node.
    ///
    /// Represents the inverse of the child node.
    pub fn not<H: Into<NodeHandle>>(&self, child: H) -> NodeHandle {
        self.nodes
            .borrow_mut()
            .insert(BuilderNode::Not(child.into()))
    }

    /// Marks a node as a "Root".
    ///
    /// Roots are the entry points of the expression. When [`ExpressionBuilder::build`]
    /// is called, only nodes accessible from these roots will be preserved.
    pub fn add_root<H: Into<NodeHandle>>(&self, root: H) {
        self.roots.borrow_mut().push(root.into());
    }

    /// Internal helper to force type errors to appear in user code.
    #[doc(hidden)]
    #[inline(always)]
    pub fn __check_type(&self) -> &Self {
        self
    }
}

// TODO: re-implement this, and get ways to remove nodes and stuff
// pub fn add_child(&mut self, parent: NodeHandle, child: NodeHandle) -> Result<(), NodeError> {
// 	if let Some(node) = self.nodes.get_mut(parent) {
// 		match node {
// 			BuilderNode::Union(kids) |
// 			BuilderNode::Intersection(kids) => {
// 				kids.push(child);
// 				Ok(())
// 			},
// 			_ => Err(NodeError::InvalidParentNodeType)
// 		}
// 	} else {
// 		Err(NodeError::InvalidParentNode)
// 	}
// }
