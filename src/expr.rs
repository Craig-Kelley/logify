use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::{HashMap, hash_map::RawEntryMut};
use rapidhash::quality::RandomState;
use serde::{Deserialize, Serialize};

mod basic;
mod convert;
mod iter;
mod ops;

/// A handle to a node within an [`Expression`].
///
/// This is a lightweight wrapper around a `u32`. It packs both the index of the node
/// and its negation state into a single integer, allowing for extremely fast copying
/// and hashing.
///
/// # Bit Layout
/// * **Bits 1..32:** The index of the node in the `Expression::nodes` vector.
/// * **Bit 0 (LSB):** The negation flag. 1 = Negated, 0 = Positive.
///
/// *Note: Because the LSB is used for negation, the maximum number of unique nodes
/// in a single Expression is `u32::MAX / 2`.*
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "fast-binary", derive(bitcode::Encode, bitcode::Decode))]
#[repr(transparent)]
pub struct NodeId(u32);

impl NodeId {
    /// Represents the empty set.
    pub const EMPTY: Self = Self(0);
    /// Represents the universal set (NOT Empty).
    pub const UNIVERSAL: Self = Self(1);
    pub(crate) const MAX: Self = Self(u32::MAX);

    pub(crate) fn new(idx: u32, neg: bool) -> Self {
        Self((idx << 1) | (if neg { 1 } else { 0 }))
    }
    pub(crate) fn raw(&self) -> u32 {
        self.0
    }
    pub(crate) fn idx(&self) -> usize {
        (self.0 >> 1) as usize
    }
    pub(crate) fn is_neg(&self) -> bool {
        (self.0 & 1) == 1
    }
    pub(crate) fn not(&self) -> Self {
        Self(self.0 ^ 1)
    }
}

/// Stores the logic or the term.
///
/// Nodes are stored in a flat vector within an [`Expression`]. Recursive structures
/// (Unions/Intersections) reference other nodes via [`NodeId`]s.
#[derive(Hash, PartialEq, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "fast-binary", derive(bitcode::Encode, bitcode::Decode))]
pub enum Node<T> {
    /// The empty set.
    /// Negation is the universal set.
    Empty,
    /// A leaf node containing a user value.
    Set(T),
    /// A logical disjunction (OR).
    Union(Vec<NodeId>),
    /// A logical conjunction (AND).
    Intersection(Vec<NodeId>),
}

/// A self-contained, optimized Boolean logic graph.
///
/// `Expression` stores logic in a deduplicated Directed Acyclic Graph (DAG). It is the
/// immutable, optimized output of an [`ExpressionBuilder`](crate::ExpressionBuilder).
///
/// # Key Features
/// * **Interning:** Every unique node (e.g., `A | B`) is stored exactly once.
/// * **Flat Memory:** Nodes are stored in a dense `Vec`, improving CPU cache locality.
/// * **Safe:** Constructed via append-only logic, ensuring no cycles exist.
///
/// # Example: Boolean Evaluation
///
/// This example builds a simple logic gate (`A AND NOT B`) and evaluates it against
/// a set of active keys.
///
/// ```rust
/// use logify::{ExpressionBuilder, Expression, Evaluator};
/// use std::collections::HashSet;
///
/// // --- 1. Build the Logic ---
/// let builder = ExpressionBuilder::new();
/// let a = builder.leaf("A");
/// let b = builder.leaf("B");
///
/// // Logic: A AND (NOT B)
/// builder.add_root(a & !b);
///
/// let expr: Expression<&str> = builder.build();
///
/// // --- 2. Define a Solver ---
/// // A simple struct that holds which keys are "Active" (True).
/// struct TruthContext(HashSet<&'static str>);
///
/// impl Evaluator<&str, bool, ()> for TruthContext {
///     // Base truths
///     fn get_universal(&mut self) -> Result<bool, ()> { Ok(true) }
///     fn get_empty(&mut self) -> Result<bool, ()> { Ok(false) }
///     
///     // Leaf lookup: True if the set contains the key
///     fn eval_set(&mut self, key: &&str) -> Result<bool, ()> {
///         Ok(self.0.contains(key))
///     }
///     
///     // Boolean Logic: OR
///     fn eval_union<'a, I>(&mut self, i: I) -> Result<bool, ()>
///         where I: IntoIterator<Item=&'a bool>, I::IntoIter: ExactSizeIterator
///     {
///         Ok(i.into_iter().any(|&b| b))
///     }
///     
///     // Boolean Logic: AND
///     fn eval_intersection<'a, I>(&mut self, i: I) -> Result<bool, ()>
///         where I: IntoIterator<Item=&'a bool>, I::IntoIter: ExactSizeIterator
///     {
///         Ok(i.into_iter().all(|&b| b))
///     }
///     
///     // Boolean Logic: AND NOT
///     fn eval_difference(&mut self, inc: &bool, exc: &bool) -> Result<bool, ()> {
///         Ok(*inc && !*exc)
///     }
/// }
///
/// // --- 3. Run Evaluation ---
/// // Case 1: "A" is present, "B" is missing. Result should be True.
/// let mut solver = TruthContext(HashSet::from(["A"]));
/// let results = expr.evaluate(&mut solver).unwrap();
/// assert_eq!(results[0], true);
///
/// // Case 2: "A" and "B" are present. Result should be False (because of NOT B).
/// let mut solver_2 = TruthContext(HashSet::from(["A", "B"]));
/// let results_2 = expr.evaluate(&mut solver_2).unwrap();
/// assert_eq!(results_2[0], false);
/// ```
#[derive(Serialize, Deserialize)]
#[serde(from = "ExpressionShadow<T>")]
#[serde(bound = "T: Serialize + for<'a> Deserialize<'a> + Hash + PartialEq")]
#[cfg_attr(feature = "fast-binary", derive(bitcode::Encode))]
pub struct Expression<T> {
    pub(crate) nodes: Vec<Node<T>>,
    pub(crate) roots: Vec<NodeId>,
    #[serde(skip, default = "default_cache")]
    #[cfg_attr(feature = "fast-binary", bitcode(skip))]
    pub(crate) cache: HashMap<NodeId, (), RandomState>,
    pub(crate) uuid: u128,
    pub(crate) generation: u64,
}

impl<T> Default for Expression<T> {
    fn default() -> Self {
        Self {
            nodes: vec![Node::Empty], // begin with Empty node 0
            roots: Vec::new(),
            cache: default_cache(),
            uuid: generate_uuid(),
            generation: 0,
        }
    }
}

impl<T: Clone + Hash + PartialEq> Clone for Expression<T> {
    fn clone(&self) -> Self {
        let nodes = self.nodes.clone();
        let cache = build_cache(&nodes);
        Self {
            nodes,
            roots: self.roots.clone(),
            cache,
            uuid: generate_uuid(),
            generation: self.generation,
        }
    }
}

fn default_cache() -> HashMap<NodeId, (), RandomState> {
    HashMap::with_hasher(RandomState::new())
}

fn generate_uuid() -> u128 {
    let low = RandomState::new();
    let mut hash_low = low.build_hasher();
    hash_low.write_usize(&low as *const _ as usize);
    let low = hash_low.finish() as u128;

    let high = RandomState::new();
    let mut hash_high = high.build_hasher();
    hash_high.write_usize(&high as *const _ as usize);
    let high = hash_high.finish() as u128;

    (high << 64) | low
}

#[derive(Deserialize)]
#[cfg_attr(feature = "fast-binary", derive(bitcode::Decode))]
struct ExpressionShadow<T> {
    nodes: Vec<Node<T>>,
    roots: Vec<NodeId>,
    uuid: u128,
    generation: u64,
}

impl<T: Hash + PartialEq> From<ExpressionShadow<T>> for Expression<T> {
    fn from(value: ExpressionShadow<T>) -> Self {
        // TODO: this won't build with the wrong location if it's in ExpressionShadow, will it?
        let cache = build_cache(&value.nodes);
        Self {
            nodes: value.nodes,
            roots: value.roots,
            cache,
            uuid: value.uuid,
            generation: value.generation,
        }
    }
}

fn build_cache<T: Hash + PartialEq>(nodes: &[Node<T>]) -> HashMap<NodeId, (), RandomState> {
    let mut cache = HashMap::with_hasher(RandomState::new());
    let hasher_builder = *cache.hasher();
    for (i, node) in nodes.iter().enumerate() {
        if let Node::Empty = node {
            continue;
        } // skip empty

        // calc hash
        let hash = hasher_builder.hash_one(node);

        // insert node into cache
        let id = NodeId::new(i as u32, false);
        // no matches because every node in a valid expression is unique
        let entry = cache.raw_entry_mut().from_hash(hash, |_| false);
        if let RawEntryMut::Vacant(entry) = entry {
            entry.insert_with_hasher(hash, id, (), |&id| {
                hasher_builder.hash_one(&nodes[id.idx()])
            });
        }
    }
    cache
}
