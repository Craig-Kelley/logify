use crate::eval::Evaluator;
use std::collections::HashSet;
use std::hash::Hash;

/// A simple evaluator for Boolean logic.
///
/// Designed for "Check" scenarios (e.g., "Does this user have permission?").
///
/// # Features
/// * **Short-Circuiting:** Unlike [`BitwiseEval`](crate::eval::bitwise_eval::BitwiseEval), this evaluator stops processing AND/OR chains
///   as soon as the result is known (e.g., `false & ...` stops immediately).
/// * **Lightweight:** No complex cloning or set allocations.
///
/// # Example
/// ```rust
/// use logify::eval::BoolEval;
/// use logify::Evaluator;
///
/// let mut ctx = BoolEval::new();
/// ctx.add("User");
/// ctx.add("Admin");
///
/// // Evaluates: User AND Admin
/// // Result: true
/// ```
#[derive(Clone)]
pub struct BoolEval<T: Hash + Eq> {
    active_keys: HashSet<T>,
}

impl<T: Hash + Eq> Default for BoolEval<T> {
    fn default() -> Self {
        Self {
            active_keys: HashSet::new(),
        }
    }
}

impl<T: Hash + Eq> BoolEval<T> {
    /// New blank `BoolEval`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks a key as "Present" (True) for the next evaluation.
    pub fn add(&mut self, key: T) {
        self.active_keys.insert(key);
    }
}

impl<T: Hash + Eq> Evaluator<T, bool, ()> for BoolEval<T> {
    fn get_universal(&mut self) -> Result<bool, ()> {
        Ok(true)
    }
    fn get_empty(&mut self) -> Result<bool, ()> {
        Ok(false)
    }

    fn eval_set(&mut self, set: &T) -> Result<bool, ()> {
        Ok(self.active_keys.contains(set))
    }

    fn eval_union<'a, I>(&mut self, values: I) -> Result<bool, ()>
    where
        I: IntoIterator<Item = &'a bool>,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(values.into_iter().any(|&v| v))
    }

    fn eval_intersection<'a, I>(&mut self, values: I) -> Result<bool, ()>
    where
        I: IntoIterator<Item = &'a bool>,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(values.into_iter().all(|&v| v))
    }

    fn eval_difference(&mut self, include: &bool, exclude: &bool) -> Result<bool, ()> {
        Ok(*include && !*exclude)
    }
}
