use std::{hash::Hash, iter::repeat_with, mem};

use hashbrown::HashMap;

use crate::{
    eval::EvaluatorCache,
    expr::{Expression, Node, NodeId},
};

impl<T: Hash + PartialEq> Expression<T> {
    /// Removes unreachable nodes (Garbage Collection).
    ///
    /// When you modify an expression (e.g., via `build_into` or manual logic), nodes that are no
    /// longer connected to any root may be left behind. This method rebuilds the expression, keeping
    /// only the live nodes.
    ///
    /// # Important
    /// * **Invalidation:** All existing [`NodeId`]s are invalidated. Do not use old IDs after calling this.
    /// * **Cache Reset:** This invalidates any attached `EvaluatorCache` (resetting its UUID).
    /// * **Reordering:** Nodes may be re-ordered in memory.
    pub fn prune<R>(self) -> Self {
        self.prune_with_cache::<()>(None)
    }

    /// Removes unreachable nodes while preserving an external cache.
    ///
    /// Identical to [`prune`](Self::prune), but attempts to remap the values inside
    /// `cache` so that expensive computations don't need to be redone.
    ///
    /// # Arguments
    /// * `cache` - The cache to update. If provided, its internal mapping is updated to match
    ///   the new node layout.
    pub fn prune_with_cache<R>(mut self, cache: Option<&mut EvaluatorCache<R>>) -> Self {
        // new expression, active nodes, and map
        let mut new_expr = Expression::new();
        let (active, max_root) = self.get_active();
        let mut map = vec![NodeId::MAX; self.nodes.len()];

        // map nodes
        for idx in 1..=max_root {
            if !active[idx] {
                continue;
            }
            let node = mem::replace(&mut self.nodes[idx], Node::Empty);
            let new_id = new_expr.map_node(node, &map);
            map[idx] = new_id;
        }

        // map roots
        for root in &self.roots {
            let id = map[root.idx()];
            let mapped = if root.is_neg() { id.not() } else { id };
            new_expr.add_root(mapped);
        }

        // remap cache
        if let Some(cache) = cache {
            new_expr.remap_cache(cache, &map, self.uuid);
        }

        new_expr
    }

    fn remap_cache<R>(&mut self, cache: &mut EvaluatorCache<R>, map: &[NodeId], from_uuid: u128) {
        // if the cache wasn't linked to the old expression, clear it to free memory
        if cache.expr_uuid != from_uuid {
            cache.clear();
            cache.expr_uuid = self.uuid;
            return;
        }

        let old_cache = &mut cache.cache;
        let mut new_cache: Vec<_> = repeat_with(|| None).take(self.nodes.len() * 2).collect();

        // take empty and universal
        if old_cache.len() >= 2 {
            new_cache[0] = old_cache[0].take();
            new_cache[1] = old_cache[1].take();
        }

        let old_cache_nodes = old_cache.len() / 2;

        for (old_idx, &new_id) in map.iter().enumerate().skip(2) {
            if old_idx >= old_cache_nodes {
                break;
            } // the old cache has no more values to be mapped
            if new_id == NodeId::MAX {
                continue;
            } // dead node

            // remap positive
            if let Some(val) = old_cache.get_mut(old_idx * 2).and_then(|r| r.take()) {
                new_cache[new_id.idx() * 2] = Some(val);
            }
            // remap negative
            if let Some(val) = old_cache.get_mut(old_idx * 2 + 1).and_then(|r| r.take()) {
                new_cache[new_id.idx() * 2 + 1] = Some(val);
            }
        }

        // replace the cache
        *old_cache = new_cache;
        cache.expr_uuid = self.uuid;
    }

    /// Moves the logic from other expressions into this one.
    ///
    /// This consumes the source expressions.
    ///
    /// # Performance
    /// * **Fast:** Operates directly on internal storage without traversing the graph.
    /// * **Dirty:** **Includes dead nodes** from the source. If the source expression contains
    ///   garbage (nodes not connected to roots), that garbage is copied into `self`.
    ///   Call [`prune`](Self::prune) afterwards if this is a concern.
    pub fn absorb_raw<I>(&mut self, exprs: I)
    where
        T: Clone,
        I: IntoIterator<Item = Expression<T>>,
    {
        for mut source in exprs {
            if source.nodes.len() == 1 {
                continue;
            }
            self.merge_raw_internal(source.nodes.len(), &source.roots, |idx| {
                mem::replace(&mut source.nodes[idx], Node::Empty)
            });
        }
    }

    /// Clones the logic from multiple expressions into this one.
    ///
    /// Useful if you need to keep the original expressions intact.
    ///
    /// # Performance
    /// * **Fast:** Linear copy of internal storage. May be slower than [`absorb_raw`](Self::absorb_raw) because it clones every term.
    /// * **Dirty:** **Includes dead nodes** from the source.
    pub fn merge_raw<'a, I>(&mut self, exprs: I)
    where
        T: 'a + Clone,
        I: IntoIterator<Item = &'a Expression<T>>,
    {
        for source in exprs {
            if source.nodes.len() == 1 {
                continue;
            }
            self.merge_raw_internal(source.nodes.len(), &source.roots, |idx| {
                source.nodes[idx].clone()
            });
        }
    }

    // updates self to hold the node and returns the nodeid
    #[inline]
    fn map_node(&mut self, node: Node<T>, map: &[NodeId]) -> NodeId {
        match node {
            Node::Empty => unreachable!(),
            Node::Set(val) => self.set(val),
            Node::Union(kids) => {
                let mapped = kids.iter().map(|k| {
                    let id = map[k.idx()];
                    if k.is_neg() { id.not() } else { id }
                });
                self.union(mapped)
            }
            Node::Intersection(kids) => {
                let mapped = kids.iter().map(|k| {
                    let id = map[k.idx()];
                    if k.is_neg() { id.not() } else { id }
                });
                self.intersection(mapped)
            }
        }
    }

    fn merge_raw_internal<F>(
        &mut self,
        source_len: usize,
        source_roots: &[NodeId],
        mut extractor: F,
    ) where
        F: FnMut(usize) -> Node<T>,
    {
        // map nodes from source -> self
        let mut map = vec![NodeId::MAX; source_len];
        for idx in 1..source_len {
            let node = extractor(idx);
            let new_id = self.map_node(node, &map);
            map[idx] = new_id;
        }

        // add roots
        for root in source_roots {
            let id = map[root.idx()];
            let mapped = if root.is_neg() { id.not() } else { id };
            self.add_root(mapped);
        }
    }

    // gets a vec with active nodes
    pub(crate) fn get_active(&self) -> (Vec<bool>, usize) {
        let mut active = vec![false; self.nodes.len()];
        let mut max_root = 0;

        // mark active roots and find the maximum root index
        for root in &self.roots {
            let idx = root.idx();
            active[idx] = true;
            if idx > max_root {
                max_root = idx;
            }
        }

        // mark all children of roots by iterating backwards
        for idx in (1..=max_root).rev() {
            if !active[idx] {
                continue;
            }
            match &self.nodes[idx] {
                Node::Union(kids) | Node::Intersection(kids) => {
                    for k in kids {
                        active[k.idx()] = true;
                    }
                }

                _ => {}
            }
        }

        // return
        (active, max_root)
    }

    pub(crate) fn absorb<F: FnMut(usize) -> Node<T>>(
        &mut self,
        active: &[bool],
        max_root: usize,
        source_roots: &[NodeId],
        mut extractor: F,
    ) {
        // map nodes from source -> self
        let mut map = vec![NodeId::MAX; max_root + 1];
        for idx in 1..=max_root {
            if !active[idx] {
                continue;
            } // skip non-active nodes
            let node = extractor(idx);
            let new_id = self.map_node(node, &map);
            map[idx] = new_id;
        }

        // add roots
        for root in source_roots {
            let id = map[root.idx()];
            let mapped = if root.is_neg() { id.not() } else { id };
            self.add_root(mapped);
        }
    }

    /// Globally deduplicates logic patterns (Common Subexpression Elimination).
    ///
    /// While the builder deduplicates nodes (e.g., `A & B` is only stored once),
    /// it does not automatically refactor deeply nested structures. `compress` finds
    /// repeated patterns across the entire graph and factors them out.
    ///
    /// # Example
    /// * **Before:** `(A & B & C)` and `(A & B & D)` are separate nodes.
    /// * **After:** `(A & B)` becomes a shared node, referenced by both parents.
    ///
    /// # Use Case
    /// Recommended to run **after** [`optimize`](Self::optimize), as optimization often exposes
    /// new structural similarities.
    pub fn compress<R>(mut self, cache: Option<&mut EvaluatorCache<R>>) -> Self {
        let starting_node_len = self.nodes.len();

        // track pair counts
        let mut pair_freq = HashMap::new();
        let mut active = vec![false; starting_node_len]; // tracks nodes with 2+ children

        // iterate via stack to count all pairs
        let mut visited = vec![false; starting_node_len];
        let mut stack = self.roots.clone();

        while let Some(id) = stack.pop() {
            if visited[id.idx()] {
                continue;
            }
            visited[id.idx()] = true;

            let node = &self.nodes[id.idx()];
            match node {
                Node::Intersection(kids) | Node::Union(kids) => {
                    stack.extend_from_slice(kids);

                    // populate pair counts
                    if kids.len() >= 2 {
                        active[id.idx()] = true;
                        let is_union = matches!(node, Node::Union(_));
                        for i in 0..kids.len() {
                            for j in (i + 1)..kids.len() {
                                let key = (kids[i], kids[j], is_union);
                                *pair_freq.entry(key).or_insert(0) += 1;
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        loop {
            let mut best_pair = (None, 1);
            for (&key, &count) in &pair_freq {
                if count > best_pair.1 {
                    best_pair = (Some(key), count);
                }
            }
            let (Some(key_best), _) = best_pair else {
                // when there's no more pairs to extract, return cleaned self
                break;
            };
            pair_freq.remove(&key_best);
            let (id_a, id_b, is_union) = key_best;

            // create the node based on the best pair
            let id_new = if is_union {
                self.union(vec![id_a, id_b])
            } else {
                self.intersection(vec![id_a, id_b])
            };

            // loop through all active nodes
            for (i, is_active) in active.iter().enumerate().take(starting_node_len) {
                if !is_active {
                    continue;
                }

                let kids = match &mut self.nodes[i] {
                    Node::Union(kids) if is_union => kids,
                    Node::Intersection(kids) if !is_union => kids,
                    _ => continue,
                };

                // if kids contain the new_id elements, replace them
                if let Ok(idx_a) = kids.binary_search(&id_a)
                    && let Ok(idx_b) = kids.binary_search(&id_b)
                {
                    // remove frequencies related to a and b
                    for &neighbor in &*kids {
                        if neighbor == id_a || neighbor == id_b {
                            continue;
                        }
                        let key_a = if id_a < neighbor {
                            (id_a, neighbor, is_union)
                        } else {
                            (neighbor, id_a, is_union)
                        };
                        if let Some(f) = pair_freq.get_mut(&key_a) {
                            *f -= 1;
                        }
                        let key_b = if id_b < neighbor {
                            (id_b, neighbor, is_union)
                        } else {
                            (neighbor, id_b, is_union)
                        };
                        if let Some(f) = pair_freq.get_mut(&key_b) {
                            *f -= 1;
                        }
                    }

                    // remove old and add new element
                    kids.remove(idx_b);
                    kids.remove(idx_a); // same location because b is after a
                    match kids.binary_search(&id_new) {
                        Ok(_) => {} // already exists in this node
                        Err(pos) => {
                            kids.insert(pos, id_new);

                            // update frequencies to include the new node
                            for &neighbor in &*kids {
                                if neighbor == id_new {
                                    continue;
                                }
                                let key_new = if id_new < neighbor {
                                    (id_new, neighbor, is_union)
                                } else {
                                    (neighbor, id_new, is_union)
                                };
                                *pair_freq.entry(key_new).or_insert(0) += 1;
                            }
                        }
                    };
                }
            }
        }

        self.clean_stack_and_remap(cache)
    }

    fn clean_stack_and_remap<R>(mut self, cache: Option<&mut EvaluatorCache<R>>) -> Self {
        let mut expr = Expression::new();

        // map self nodes -> new_expr nodes
        let mut map = vec![NodeId::MAX; self.nodes.len()];
        map[0] = NodeId::EMPTY;

        // loop through each root
        let mut stack = Vec::new();
        for &root in &self.roots {
            // check if root is already processed
            if map[root.idx()] != NodeId::MAX {
                let id = map[root.idx()];
                let mapped = if root.is_neg() { id.not() } else { id };
                expr.add_root(mapped);
                continue;
            }

            // process root
            stack.clear();
            stack.push((root, false));
            while let Some((id, visited)) = stack.pop() {
                let idx = id.idx();
                if map[idx] != NodeId::MAX {
                    continue;
                } // skip already processed nodes

                if visited {
                    // children processed, construct node in target
                    let node = mem::replace(&mut self.nodes[idx], Node::Empty);
                    let new_id = expr.map_node(node, &map);
                    map[idx] = new_id;
                } else {
                    // mark as visited, to process after children are processed
                    stack.push((id, true));
                    match &self.nodes[idx] {
                        Node::Union(kids) | Node::Intersection(kids) => {
                            for kid in kids.iter().rev() {
                                if map[kid.idx()] == NodeId::MAX {
                                    stack.push((*kid, false));
                                }
                            }
                        }
                        _ => {} // no children to push
                    }
                }
            }

            // add processed root to target
            let root_id = map[root.idx()];
            let mapped = if root.is_neg() {
                root_id.not()
            } else {
                root_id
            };
            expr.add_root(mapped);
        }

        // remap cache
        if let Some(cache) = cache {
            expr.remap_cache(cache, &map, self.uuid);
        }

        expr
    }
}
