use crate::eval::Evaluator;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::{BitAndAssign, BitOrAssign, Sub};

/// A generic solver for types that behave like mathematical sets.
///
/// This struct allows you to evaluate logic against data structures like `HashSet`, `BTreeSet`,
/// `BitVec`, or `RoaringBitmap`.
///
/// # Logic Semantics
/// * **Variables:** Treated as transient input. They are **removed** from the solver during evaluation
///   to avoid unnecessary cloning.
/// * **Universal Set:** Treated as persistent context. It is **cloned** (not consumed), so large
///   structures should be wrapped in `Arc` or `Rc`.
/// * **Operations:** Uses in-place mutation (`|=`, `&=`) to minimize memory allocation overhead
///   during unions and intersections.
///
/// # Example: HashSet
/// ```rust
/// use logify::eval::BitwiseEval;
/// use logify::Evaluator;
/// use std::collections::HashSet;
///
/// // Define the "Universe" (All items)
/// let universal = HashSet::from([1, 2, 3, 4, 5]);
///
/// // Create solver
/// let mut solver = BitwiseEval::new(universal);
///
/// // Add data: "TagA" has items {1, 2}
/// solver.insert("TagA", HashSet::from([1, 2]));
///
/// // Add data: "TagB" has items {2, 3}
/// solver.insert("TagB", HashSet::from([2, 3]));
///
/// // Logic would correspond to: TagA OR TagB
/// // Result: {1, 2, 3}
/// ```
#[derive(Clone)]
pub struct BitwiseEval<K, S> {
    pub variables: HashMap<K, S>,
    pub universal: S,
}

impl<K, S> BitwiseEval<K, S> {
    /// Creates a new solver with the given Universal set.
    ///
    /// The `universal` set is returned when evaluating `NOT Empty`, or when
    /// an empty intersection occurs (depending on logic rules).
    pub fn new(universal: S) -> Self {
        Self {
            variables: HashMap::new(),
            universal,
        }
    }

    /// Registers a variable for the next evaluation.
    ///
    /// *Note: The value is moved into the solver and will be consumed (removed)
    /// when the matching leaf node is evaluated.*
    pub fn insert(&mut self, key: K, value: S)
    where
        K: Hash + Eq,
    {
        self.variables.insert(key, value);
    }
}

impl<K, S> Evaluator<K, S, ()> for BitwiseEval<K, S>
where
    K: Hash + Eq,
    S: Default + Clone,
    for<'a> S: BitOrAssign<&'a S> + BitAndAssign<&'a S>,
    for<'a> &'a S: Sub<Output = S>,
{
    fn get_universal(&mut self) -> Result<S, ()> {
        Ok(self.universal.clone())
    }

    fn get_empty(&mut self) -> Result<S, ()> {
        Ok(S::default())
    }

    fn eval_set(&mut self, key: &K) -> Result<S, ()> {
        Ok(self.variables.remove(key).unwrap_or_default())
    }

    fn eval_union<'a, I>(&mut self, values: I) -> Result<S, ()>
    where
        S: 'a,
        I: IntoIterator<Item = &'a S>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = values.into_iter();
        let mut result = iter.next().unwrap().clone();
        for item in iter {
            result |= item;
        }
        Ok(result)
    }

    fn eval_intersection<'a, I>(&mut self, values: I) -> Result<S, ()>
    where
        S: 'a,
        I: IntoIterator<Item = &'a S>,
        I::IntoIter: ExactSizeIterator,
    {
        let mut iter = values.into_iter();
        let mut result = iter.next().unwrap().clone();
        for item in iter {
            result &= item;
        }
        Ok(result)
    }

    fn eval_difference(&mut self, include: &S, exclude: &S) -> Result<S, ()> {
        Ok(include - exclude)
    }
}
