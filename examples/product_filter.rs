use std::collections::{HashMap, HashSet};

use logify::{Evaluator, ExpressionBuilder};

// product database
struct ProductDb {
    tags: HashMap<&'static str, HashSet<i32>>,
    all_products: HashSet<i32>,
}

impl ProductDb {
    fn new() -> Self {
        Self {
            tags: HashMap::new(),
            all_products: HashSet::from([1, 2, 3, 4, 5]),
        }
    }

    fn add_tag(&mut self, tag: &'static str, ids: &[i32]) {
        self.tags
            .insert(tag, HashSet::from_iter(ids.iter().copied()));
    }
}

impl Evaluator<&str, HashSet<i32>, ()> for ProductDb {
    fn get_universal(&mut self) -> Result<HashSet<i32>, ()> {
        Ok(self.all_products.clone())
    }

    fn get_empty(&mut self) -> Result<HashSet<i32>, ()> {
        Ok(HashSet::new())
    }

    fn eval_set(&mut self, key: &&str) -> Result<HashSet<i32>, ()> {
        // Return the set if it exists, or empty if not
        Ok(self.tags.get(key).cloned().unwrap_or_default())
    }

    fn eval_union<'a, I>(&mut self, values: I) -> Result<HashSet<i32>, ()>
    where
        I: IntoIterator<Item = &'a HashSet<i32>>,
        I::IntoIter: ExactSizeIterator,
    {
        // Manual Union Logic: Create empty, extend with others
        let mut result = HashSet::new();
        for set in values {
            result.extend(set);
        }
        Ok(result)
    }

    fn eval_intersection<'a, I>(&mut self, values: I) -> Result<HashSet<i32>, ()>
    where
        I: IntoIterator<Item = &'a HashSet<i32>>,
        I::IntoIter: ExactSizeIterator,
    {
        // Manual Intersection Logic
        let mut iter = values.into_iter();
        if let Some(first) = iter.next() {
            let mut result = first.clone();
            for set in iter {
                result.retain(|id| set.contains(id));
            }
            Ok(result)
        } else {
            Ok(self.all_products.clone())
        }
    }

    fn eval_difference(
        &mut self,
        include: &HashSet<i32>,
        exclude: &HashSet<i32>,
    ) -> Result<HashSet<i32>, ()> {
        Ok(include - exclude)
    }
}

fn main() -> Result<(), ()> {
    // 1. Build logic
    let builder = ExpressionBuilder::new();
    let red = builder.leaf("Red");
    let blue = builder.leaf("Blue");
    let expensive = builder.leaf("Expensive");

    let filter = (red | blue) & !expensive;
    builder.add_root(filter);

    let expr = builder.build();

    // 2. Setup Evaluator
    let mut db = ProductDb::new();
    db.add_tag("Red", &[1, 2]);
    db.add_tag("Blue", &[3, 4]);
    db.add_tag("Expensive", &[1, 4]);

    // 5. Evaluate
    let results = expr.evaluate(&mut db)?;

    let mut matching_products: Vec<i32> = results[0].iter().copied().collect();
    matching_products.sort();

    println!("Matching Products: {:?}", matching_products);
    assert_eq!(matching_products, vec![2, 3]);

    Ok(())
}
