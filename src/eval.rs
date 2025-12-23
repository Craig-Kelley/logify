use crate::expr::{Expression, Node};

mod bitwise_eval;
pub use bitwise_eval::BitwiseEval;
mod bool_eval;
pub use bool_eval::BoolEval;
use serde::{Deserialize, Serialize};

/// Defines how to resolve abstract logic into concrete results.
///
/// To run an [`Expression`], you must implement this trait. It acts as the bridge
/// between the boolean logic graph and your specific domain (e.g., SQL generation, bitmask operations,
/// search engine query execution).
///
/// # Type Parameters
/// * `T`: The **Term** type used in the expression (e.g., `String` for tags, `u32` for IDs).
/// * `R`: The **Result** type produced by the evaluation (e.g., `Vec<i32>`, `RoaringBitmap`, `SqlFragment`).
/// * `E`: The **Error** type that can occur during evaluation.
///
/// # Optimization Note
/// This trait uses `eval_difference` instead of a direct `not` method. This allows implementations
/// to avoid calculating "Everything except X" (which is often expensive or infinite) and instead
/// implicitly calculate `A AND NOT B`.
pub trait Evaluator<T, R, E> {
    /// Returns the Universal Set (The set of all things).
    ///
    /// This is used when the expression resolves to a pure negation (e.g., `!A`).
    /// To resolve `!A`, the library calculates `Universal - A`.
    ///
    /// If your domain does not support a "Universal" set (e.g., an infinite number line),
    /// you can return an error here, but be aware that top-level negations will fail.
    fn get_universal(&mut self) -> Result<R, E>; // TODO: Might not be useful

    /// Returns the Empty Set (The set of nothing).
    fn get_empty(&mut self) -> Result<R, E>;

    /// Resolves a single leaf node value into a result.
    ///
    /// # Example
    /// If `T` is a User ID, this might look up that user in a database and return a `Result`
    /// containing that user's permissions.
    fn eval_set(&mut self, set: &T) -> Result<R, E>;

    /// merges multiple results via a Union (OR) operation.
    ///
    /// # Arguments
    /// * `values` - An iterator of results previously computed by `eval_set`, `eval_intersection`, etc.
    ///
    /// # Expected Behavior
    /// Return a result containing items present in **at least one** of the input values.
    fn eval_union<'a, I>(&mut self, values: I) -> Result<R, E>
    where
        R: 'a,
        I: IntoIterator<Item = &'a R>,
        I::IntoIter: ExactSizeIterator;

    /// Filters multiple results via an Intersection (AND) operation.
    ///
    /// # Arguments
    /// * `values` - An iterator of results previously computed by `eval_set`, `eval_intersection`, etc.
    ///
    /// # Expected Behavior
    /// Return a result containing only items present in **all** of the input values.
    fn eval_intersection<'a, I>(&mut self, values: I) -> Result<R, E>
    where
        R: 'a,
        I: IntoIterator<Item = &'a R>,
        I::IntoIter: ExactSizeIterator;

    /// Calculates the difference between two results (`Include AND NOT Exclude`).
    ///
    /// This is used to handle negation. The expression engine transforms negations
    /// into difference operations where possible to avoid materializing the Universal set.
    ///
    /// * `!A` becomes `eval_difference(Universal, A)`
    /// * `A & !B` becomes `eval_difference(A, B)`
    ///
    /// # Arguments
    /// * `include` - The base set of items.
    /// * `exclude` - The set of items to remove from the base set.
    fn eval_difference(&mut self, include: &R, exclude: &R) -> Result<R, E>;
}

/// A reusable memory buffer for expression evaluation.
///
/// When evaluating an expression multiple times (e.g., against different rows in a database),
/// allocating new vectors for every calculation is inefficient. `EvaluatorCache` holds onto
/// the allocated memory between runs.
///
/// Use this to avoid repeated allocations when evaluating the same expression multiple times.
///
/// # Automatic Invalidation
/// This struct stores a version UUID of the expression it was last used with. If you pass
/// this cache to `evaluate_with` on a modified or completely different expression, it will
/// automatically detect the mismatch and clear itself.
///
/// # Memory & Performance
/// * **Allocations:** Reuses internal vectors to minimize heap traffic.
/// * **Cloning:** When `evaluate_with` returns, the final results for the roots are **cloned**
///   from this cache.
///   * If your result type `R` is large (e.g., a 10MB Bitmap or large Vector), this clone
///     can be expensive.
///   * **Recommendation:** Wrap large results in [`std::sync::Arc`] or [`std::rc::Rc`] so that
///     cloning is cheap (pointer copy) rather than deep.
///
/// # Example
/// ```rust
/// use logify::{EvaluatorCache, ExpressionBuilder, Evaluator};
/// # // Mock Setup (Hidden from docs)
/// # struct Solver;
/// # impl Evaluator<&str, bool, ()> for Solver {
/// #     fn get_universal(&mut self) -> Result<bool, ()> { Ok(true) }
/// #     fn get_empty(&mut self) -> Result<bool, ()> { Ok(false) }
/// #     fn eval_set(&mut self, _: &&str) -> Result<bool, ()> { Ok(true) }
/// #     fn eval_union<'a, I>(&mut self, _: I) -> Result<bool, ()> where I: IntoIterator<Item=&'a bool>, I::IntoIter: ExactSizeIterator { Ok(true) }
/// #     fn eval_intersection<'a, I>(&mut self, _: I) -> Result<bool, ()> where I: IntoIterator<Item=&'a bool>, I::IntoIter: ExactSizeIterator { Ok(true) }
/// #     fn eval_difference(&mut self, _: &bool, _: &bool) -> Result<bool, ()> { Ok(true) }
/// # }
///
/// // Setup
/// let mut cache = EvaluatorCache::new();
/// let mut solver = Solver;
///
/// // Build a simple expression
/// let builder = ExpressionBuilder::new();
/// builder.add_root(builder.leaf("A"));
/// let expr = builder.build();
///
/// let dataset = vec!["Row1", "Row2", "Row3"];
///
/// // Fast: Reuses the same vectors for every iteration
/// for item in dataset {
///     // In a real scenario, you would update the roots or append expressions each time.
///     let result = expr.evaluate_with(&mut solver, &mut cache);
/// }
/// ```
#[cfg_attr(feature = "fast-binary", derive(bitcode::Encode, bitcode::Decode))]
#[derive(Serialize, Deserialize)]
pub struct EvaluatorCache<R> {
    pub(crate) cache: Vec<Option<R>>,
    pub(crate) include_indices: Vec<usize>,
    pub(crate) exclude_indices: Vec<usize>,
    pub(crate) expr_uuid: u128, // 0 for an uninitialized cache
}

impl<R> Default for EvaluatorCache<R> {
    fn default() -> Self {
        Self {
            cache: Vec::new(),
            include_indices: Vec::new(),
            exclude_indices: Vec::new(),
            expr_uuid: 0,
        }
    }
}

impl<R> EvaluatorCache<R> {
    /// Creates a new, empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Manually clears the internal buffers and resets the versioning.
    ///
    /// Usually not necessary, as `evaluate_with` handles invalidation automatically.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.expr_uuid = 0; // mark as uninitialized
    }
}

impl<T> Expression<T> {
    /// Evaluates the expression using a temporary cache.
    ///
    /// This is a convenience wrapper around [`evaluate_with`](Self::evaluate_with).
    /// It creates a fresh `EvaluatorCache`, runs the evaluation, and then drops the cache.
    ///
    /// # Performance Note
    /// Because this allocates memory for every call, it is not recommended for tight loops.
    /// Use `evaluate_with` for repeated evaluations.
    pub fn evaluate<R, E, S>(&self, solver: &mut S) -> Result<Vec<R>, E>
    where
        R: Clone,
        S: Evaluator<T, R, E>,
    {
        let mut cache = EvaluatorCache::new();
        self.evaluate_with(solver, &mut cache)
    }

    /// Evaluates the expression using a persistent, external cache.
    ///
    /// This is the most efficient way to evaluate an expression multiple times.
    ///
    /// # How it works
    /// 1. **Validation:** Checks if the `cache` matches the current expression's UUID. If not, it clears the cache.
    /// 2. **Execution:** Iterates through the node graph. Intermediate results are stored in the cache.
    /// 3. **Reuse:** If called again, the internal vectors (`Vec<Option<R>>`) are reused, preventing heap allocation overhead.
    ///
    /// # Cache Invalidation
    /// The cache is tied to the structure of the expression. Modifying the expression
    /// (e.g., via `compress()`, `prune()`, or `optimize()`) changes the UUID, causing
    /// the cache to reset on the next call.
    pub fn evaluate_with<R, E, S>(
        &self,
        solver: &mut S,
        cache: &mut EvaluatorCache<R>,
    ) -> Result<Vec<R>, E>
    where
        R: Clone,
        S: Evaluator<T, R, E>,
    {
        // cache validation
        if cache.expr_uuid != self.uuid {
            cache.clear();
            cache.expr_uuid = self.uuid;
        }

        // load cache
        let cache_vec = &mut cache.cache;
        if cache_vec.len() < self.nodes.len() * 2 {
            cache_vec.resize(self.nodes.len() * 2, None);
        }

        // initialize active nodes with the roots to find
        let mut max_root = 0; // furthest root location, node 0 has no children, so safe as a flag to avoid finding children
        let mut active = vec![false; self.nodes.len()];
        for root in &self.roots {
            // skip over already loaded roots
            if cache_vec[root.idx() << 1].is_none() {
                active[root.idx()] = true;
                if root.idx() > max_root {
                    max_root = root.idx();
                }
            }
        }

        // finds all children of uncomputed roots
        if max_root != 0 {
            for idx in (0..self.nodes.len()).rev() {
                if !active[idx] {
                    continue;
                } // dead node
                // activate all children
                match &self.nodes[idx] {
                    Node::Union(kids) | Node::Intersection(kids) => {
                        for k in kids {
                            active[k.idx()] = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        // evaluate each node
        for (idx, node) in self.nodes.iter().enumerate() {
            if idx > max_root {
                break;
            } // only evaluate up to the last needed root
            if !active[idx] {
                continue;
            } // skips non-active nodes
            if cache_vec[idx << 1].is_some() {
                continue;
            } // already evaluated

            // node must be calculated
            let result = Self::evaluate_node(
                node,
                solver,
                cache_vec,
                &mut cache.include_indices,
                &mut cache.exclude_indices,
            )?;
            cache_vec[idx << 1] = Some(result);
        }

        // all root positives are now in cache
        let mut results = Vec::with_capacity(self.roots.len());
        for root in &self.roots {
            if let Some(res) = &cache_vec[root.raw() as usize] {
                results.push(res.clone());
            } else {
                if cache_vec[1].is_none() {
                    cache_vec[1] = Some(solver.get_universal()?);
                }
                let uni = cache_vec[1].as_ref().unwrap();
                if root.raw() == 1 {
                    results.push(uni.clone());
                } else {
                    let pos = cache_vec[root.idx() << 1].as_ref().unwrap();
                    let neg = solver.eval_difference(uni, pos)?;
                    cache_vec[root.raw() as usize] = Some(neg.clone());
                    results.push(neg);
                }
            }
        }
        Ok(results)
    }

    /// Evaluates the expression while aggressively freeing memory.
    ///
    /// Unlike standard evaluation, which keeps all intermediate results until the end,
    /// this method calculates reference counts for every node. As soon as a node's
    /// result is consumed by all its parents, the memory is dropped.
    ///
    /// # Trade-offs
    /// * **Pros:** Significantly lower peak memory usage. Ideal for very large result types (e.g., Bitmaps, Images).
    /// * **Cons:** Slower execution speed due to the overhead of calculating reference counts and dropping values during iteration.
    pub fn evaluate_with_pruning<R, E, S>(&self, solver: &mut S) -> Result<Vec<R>, E>
    where
        R: Clone,
        S: Evaluator<T, R, E>,
    {
        // create cache
        let mut cache = vec![None; self.nodes.len() * 2];
        let mut include_indices = Vec::new();
        let mut exclude_indices = Vec::new();

        // construct the counts
        let mut counts = vec![0; self.nodes.len()];
        for &root in &self.roots {
            // retain roots until the end
            counts[root.idx()] += 1;
        }
        for idx in (0..self.nodes.len()).rev() {
            if counts[idx] == 0 {
                continue;
            } // dead node
            match &self.nodes[idx] {
                Node::Union(kids) | Node::Intersection(kids) => {
                    for k in kids {
                        counts[k.idx()] += 1;
                    }
                }
                _ => {}
            }
        }

        // traverse the expression linearly
        for (idx, node) in self.nodes.iter().enumerate() {
            if counts[idx] == 0 {
                continue;
            } // node isn't used
            if cache[idx << 1].is_some() {
                continue;
            } // already evaluated

            // node must be calculated
            let result = Self::evaluate_node(
                node,
                solver,
                &mut cache,
                &mut include_indices,
                &mut exclude_indices,
            )?;
            cache[idx << 1] = Some(result);

            // decrement and remove cache if there are no more parents
            match node {
                Node::Union(kids) | Node::Intersection(kids) => {
                    for k in kids {
                        counts[k.idx()] -= 1;
                        if counts[k.idx()] == 0 {
                            cache[k.idx() << 1] = None;
                            cache[(k.idx() << 1) + 1] = None;
                        }
                    }
                }
                _ => {}
            }
        }

        // all root positives are now in cache
        let mut results = Vec::with_capacity(self.roots.len());
        for root in &self.roots {
            if let Some(res) = &cache[root.raw() as usize] {
                // root in cache
                results.push(res.clone());
            } else {
                // root not in cache, must be negative and positive must be in cache
                if cache[1].is_none() {
                    cache[1] = Some(solver.get_universal()?);
                }
                let uni = cache[1].as_ref().unwrap();
                if root.raw() == 1 {
                    results.push(uni.clone());
                } else {
                    let pos = cache[root.idx() << 1].as_ref().unwrap();
                    let neg = solver.eval_difference(uni, pos)?;
                    cache[root.raw() as usize] = Some(neg.clone());
                    results.push(neg);
                }
            }
        }
        Ok(results)
    }

    #[inline]
    fn evaluate_node<R, E, S>(
        node: &Node<T>,
        solver: &mut S,
        cache_vec: &mut [Option<R>],
        include_indices: &mut Vec<usize>,
        exclude_indices: &mut Vec<usize>,
    ) -> Result<R, E>
    where
        R: Clone,
        S: Evaluator<T, R, E>,
    {
        match node {
            Node::Empty => Ok(solver.get_empty()?),
            Node::Set(set) => Ok(solver.eval_set(set)?),
            Node::Union(kids) => {
                // make sure all negated terms are calculated
                let (uni_cache, other_cache) = cache_vec.split_at_mut(2);
                for k in kids {
                    let idx = k.raw() as usize - 2;
                    let pos_idx = (k.idx() << 1) - 2;
                    if other_cache[idx].is_none() {
                        // must be negative
                        let uni = uni_cache[1].get_or_insert(solver.get_universal()?);
                        let pos = other_cache[pos_idx].as_ref().unwrap();
                        let neg = solver.eval_difference(uni, pos)?;
                        other_cache[idx] = Some(neg); // add negative to cache
                    }
                }
                // evaluate the union
                Ok(solver.eval_union(
                    kids.iter()
                        .map(|k| cache_vec[k.raw() as usize].as_ref().unwrap()),
                )?)
            }
            Node::Intersection(kids) => {
                // A&B&C'&D' == (A&B)-(C|D)
                include_indices.clear();
                exclude_indices.clear();
                for k in kids {
                    if k.is_neg() {
                        if cache_vec[k.raw() as usize].is_some() {
                            // & is faster, so if the negative is computed, include it
                            include_indices.push(k.raw() as usize);
                        } else {
                            // negative is not computed, so exclude the positive
                            exclude_indices.push(k.idx() << 1);
                        }
                    } else {
                        // k is positive, include it
                        include_indices.push(k.raw() as usize);
                    }
                }

                // intersections must have at least two terms
                if exclude_indices.is_empty() {
                    // no exclusions so use the include as the result
                    let include = solver.eval_intersection(
                        include_indices
                            .iter()
                            .map(|&i| cache_vec[i].as_ref().unwrap()),
                    )?;
                    Ok(include)
                } else {
                    // get include
                    let include = if include_indices.is_empty() {
                        // use universe if no inclusions are present
                        if cache_vec[1].is_none() {
                            cache_vec[1] = Some(solver.get_universal()?);
                        }
                        cache_vec[1].as_ref().unwrap()
                    } else if include_indices.len() == 1 {
                        cache_vec[include_indices[0]].as_ref().unwrap()
                    } else {
                        &solver.eval_intersection(
                            include_indices
                                .iter()
                                .map(|&i| cache_vec[i].as_ref().unwrap()),
                        )?
                    };

                    // get exclude (must be more than 1)
                    let exclude = if exclude_indices.len() == 1 {
                        cache_vec[exclude_indices[0]].as_ref().unwrap()
                    } else {
                        &solver.eval_union(
                            exclude_indices
                                .iter()
                                .map(|&i| cache_vec[i].as_ref().unwrap()),
                        )?
                    };

                    // compute difference
                    Ok(solver.eval_difference(include, exclude)?)
                }
            }
        }
    }
}
