use std::hash::Hash;

use crate::{
    expr::{Expression, Node, NodeId},
    opt::merger::{MergeRelation, MergeResult, Mergeable, Merger},
};

impl<T: Hash + PartialEq> Expression<T> {
    pub(super) fn apply_logic_reduction<M: Mergeable<T>>(
        &mut self,
        mut kids: Vec<NodeId>,
        is_union: bool,
        merger: &mut Merger<T, M>,
        merger_depth: usize,
    ) -> NodeId {
        // De Morgan's
        let should_flip = if is_union {
            // if any element of a union is negative, can standardize and possibly avoid U-A via intersection
            // example: (A|B|C|D') = (A'&B'&C'&D)' == U-(D-(A|B|C)) // simple standardization
            // (A|B|C|D'|E') = (A'&B'&C'&D&E)' = U-((D&E)-(A|B|C)) // saved U-D and U-E for a single U-_
            kids.iter().any(|k| k.is_neg())
        } else {
            // if all elements of an intersection are negative, save U-X
            // example:
            // (A'&B')' == U-(U-(A|B)) eval
            //  = (A|B) == (A|B) eval
            // for non-negated intersections, the cost is equivalent
            kids.iter().all(|k| k.is_neg())
        };
        if should_flip {
            let flipped_kids = kids.iter().map(|k| k.not()).collect();
            return self
                .apply_logic_reduction(flipped_kids, !is_union, merger, merger_depth)
                .not();
        }

        // flattening, A | (B | C) == A | B | C
        let mut flat_kids = Vec::with_capacity(kids.len() + 1); // at least kids.len() items, with an extra for appending to the end
        for k in kids {
            // if child is same type, it can be flattened
            let same_type = !k.is_neg()
                && match (&self.nodes[k.idx()], is_union) {
                    // TODO: ignores negations?
                    (Node::Union(_), true) => true,
                    (Node::Intersection(_), false) => true,
                    _ => false,
                };
            if same_type {
                match &self.nodes[k.idx()] {
                    Node::Union(g) | Node::Intersection(g) => flat_kids.extend(g.clone()), // add grandkids to own kids
                    _ => unreachable!(),
                }
            } else {
                flat_kids.push(k);
            }
        }
        kids = flat_kids;

        if kids.len() >= 2 {
            // absorption A & (A & B)' => A & B'
            let mut i = 0;
            while i < kids.len() {
                let id_a = kids[i];
                let is_a_set = matches!(self.nodes[id_a.idx()], Node::Set(_));
                if is_a_set {
                    let mut j = 0;
                    while j < kids.len() {
                        if i == j {
                            j += 1;
                            continue;
                        }
                        let id_b = kids[j];

                        // get b's type and children
                        let (b_is_union, b_kids) = match &self.nodes[id_b.idx()] {
                            Node::Union(gk) => (!id_b.is_neg(), gk),
                            Node::Intersection(gk) => (id_b.is_neg(), gk),
                            _ => {
                                j += 1;
                                continue;
                            }
                        };

                        // only care when we have diff ops, A & (|) or A | (&)
                        if b_is_union == is_union {
                            j += 1;
                            continue;
                        }

                        // iterate through before begining allocation, as it's likely to not change, and cache will make change_b == true O(1) lookup for already iterated terms
                        let change_b = b_kids.iter().any(|&b_k| {
                            let effective_k = if id_b.is_neg() { b_k.not() } else { b_k };
                            let rel = merger.get_relation(self, id_a, effective_k, merger_depth);
                            if !is_union {
                                rel.is_disjoint()
                            } else {
                                rel.is_cover()
                            }
                        });
                        // if b needs to be changed
                        if change_b {
                            let mut new_b_kids = Vec::new();
                            for &b_k in b_kids {
                                let effective_k = if id_b.is_neg() { b_k.not() } else { b_k };
                                let rel =
                                    merger.get_relation(self, id_a, effective_k, merger_depth);
                                let should_remove = if !is_union {
                                    rel.is_disjoint()
                                } else {
                                    rel.is_cover()
                                };
                                if !should_remove {
                                    // for A&(A&B)', save B' instead of B
                                    new_b_kids.push(effective_k);
                                }
                            }
                            let new_b_id = if b_is_union {
                                self.union(new_b_kids)
                            } else {
                                self.intersection(new_b_kids)
                            };

                            kids[j] = new_b_id;
                        }
                        j += 1;
                    }
                }
                i += 1;
            }

            // relationship reduction O(N^2)
            let mut i = 0;
            while i < kids.len() {
                // if i >= kids.len() { break; }
                let mut j = i + 1;
                while j < kids.len() {
                    let id_a = kids[i];
                    let id_b = kids[j];

                    // check relation
                    let rel = merger.get_relation(self, id_a, id_b, merger_depth);
                    // true = node i, false = node j
                    let changed = match (rel, is_union) {
                        (MergeRelation::EQUAL, _) => {
                            kids.swap_remove(j);
                            Some(false)
                        } // A == B, rem j
                        (r, false) if r.is_disjoint() => return NodeId::EMPTY,
                        (r, true) if r.is_cover() => return NodeId::UNIVERSAL,
                        (r, true) if r.is_subset() => {
                            kids.swap_remove(i);
                            Some(true)
                        }
                        (r, false) if r.is_subset() => {
                            kids.swap_remove(j);
                            Some(false)
                        }
                        (r, true) if r.is_superset() => {
                            kids.swap_remove(j);
                            Some(false)
                        }
                        (r, false) if r.is_superset() => {
                            kids.swap_remove(i);
                            Some(true)
                        }
                        // TODO: option to not re-check items when a merge fails (would be useful for things like a certain type being able to merge only with the same type, then we aren't rechecking if a type can merge with some other type)
                        // TODO: just make sure this wont effect something like EMPTY turning the entire thing into EMPTY (such that it no longer does that)
                        // no relation was found, run a merge check
                        _ =>
                        // if both are sets
                        {
                            if let (Node::Set(a), Node::Set(b)) =
                                (&self.nodes[id_a.idx()], &self.nodes[id_b.idx()])
                            {
                                let neg_a = id_a.is_neg();
                                let neg_b = id_b.is_neg();

                                // get the merged node if it can be merged
                                let merged = if is_union {
                                    merger.mergeable.merge_union(a, neg_a, b, neg_b)
                                } else {
                                    merger.mergeable.merge_intersection(a, neg_a, b, neg_b)
                                };
                                if let Some(res) = merged {
                                    // get new node id
                                    let new_id = match res {
                                        MergeResult::Empty => NodeId::EMPTY,
                                        MergeResult::Universal => NodeId::UNIVERSAL,
                                        MergeResult::Set(set, is_neg) => {
                                            let id = self.set(set);
                                            if is_neg { id.not() } else { id }
                                        }
                                    };

                                    // j merged into i
                                    kids[i] = new_id; // update i
                                    kids.swap_remove(j); // remove B
                                    Some(true) // i changed
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                    };

                    // loop control
                    if let Some(changed_node) = changed {
                        if changed_node {
                            // i was changed
                            j = i + 1; // recheck all of j against the new i
                        }
                        // if j was changed, don't increment, to recheck it
                    } else {
                        j += 1; // continue loop as normal
                    }
                }
                i += 1;
            }

            // attempt factoring
            // note: factoring intersections may result in harder evaluations (no early returns in unions), so stick to union factoring
            if is_union && let Some(factored) = self.try_factoring(&kids) {
                return factored;
            }
        }

        // return
        if is_union {
            self.union(kids)
        } else {
            self.intersection(kids)
        }
    }

    // NOTE: only handles unions of intersections/sets
    fn try_factoring(&mut self, kids: &[NodeId]) -> Option<NodeId> {
        // loops through each child
        for i in 0..kids.len() {
            let owned_i;
            let kids_i = match &self.nodes[kids[i].idx()] {
                Node::Intersection(children) if !kids[i].is_neg() => children,
                Node::Union(children) if kids[i].is_neg() => {
                    owned_i = children.iter().map(|id| id.not()).collect();
                    &owned_i
                }
                _ => continue, // ignore Node::Set(), handled in Merger absorption
            };

            for j in (i + 1)..kids.len() {
                let owned_j;
                let kids_j = match &self.nodes[kids[j].idx()] {
                    Node::Intersection(children) if !kids[j].is_neg() => children,
                    Node::Union(children) if kids[j].is_neg() => {
                        owned_j = children.iter().map(|id| id.not()).collect();
                        &owned_j
                    }
                    _ => continue, // ignore Node::Set(), handled in Merger absorption
                };

                // collect common terms
                let mut common = Vec::new(); // TODO: capacity?
                let mut p_i = 0;
                let mut p_j = 0;
                while p_i < kids_i.len() && p_j < kids_j.len() {
                    if kids_i[p_i] == kids_j[p_j] {
                        common.push(kids_i[p_i]);
                        p_i += 1;
                        p_j += 1;
                    } else if kids_i[p_i] < kids_j[p_j] {
                        p_i += 1;
                    } else {
                        p_j += 1;
                    }
                }

                // if a match was found, (A & B) | (A & C) => A & (B|C)
                if !common.is_empty() {
                    // TODO: faster check because they SHOULD? be sorted already
                    // residuals
                    let mut res_i = kids_i.clone();
                    res_i.retain(|x| !common.contains(x));
                    let mut res_j = kids_j.clone();
                    res_j.retain(|x| !common.contains(x));

                    // allocate residuals
                    let res_id_i = if res_i.is_empty() {
                        NodeId::UNIVERSAL
                    } else {
                        self.intersection(res_i)
                    };
                    let res_id_j = if res_j.is_empty() {
                        NodeId::UNIVERSAL
                    } else {
                        self.intersection(res_j)
                    };

                    let common_id = self.intersection(common);
                    let residuals_id = self.union(vec![res_id_i, res_id_j]);
                    let new_node = self.intersection(vec![common_id, residuals_id]);

                    // create the old list with the new node made from two nodes
                    let mut new_kids = Vec::with_capacity(kids.len() - 1);
                    new_kids.push(new_node);
                    for (idx, &id) in kids.iter().enumerate() {
                        if idx != i && idx != j {
                            new_kids.push(id);
                        }
                    }
                    return Some(self.union(new_kids));
                }
            }
        }
        None
    }
}
