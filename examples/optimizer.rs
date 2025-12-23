use std::fmt::Display;

use logify::{
    ExpressionBuilder, logic,
    opt::{Mergeable, OptimizerConfig, SetRelation},
};

// Geographical locations
#[derive(PartialEq, Hash, Clone, Debug)]
enum Geo {
    USA,
    California,
    Texas,
    France,
    Paris,
}

impl Display for Geo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loc = match self {
            Geo::USA => "USA",
            Geo::California => "California",
            Geo::Texas => "Texas",
            Geo::France => "France",
            Geo::Paris => "Paris",
        };
        write!(f, "{}", loc)
    }
}

// Define the relationships
struct GeoMerger;

impl Mergeable<Geo> for GeoMerger {
    fn get_relation(&mut self, a: &Geo, b: &Geo) -> SetRelation {
        // mark subset as inside of, and superset as contains
        match (a, b) {
            (Geo::California, Geo::USA) => SetRelation::Subset,
            (Geo::Texas, Geo::USA) => SetRelation::Subset,
            (Geo::Paris, Geo::France) => SetRelation::Subset,

            (Geo::USA, Geo::California) => SetRelation::Superset,

            (Geo::Texas, Geo::France) => SetRelation::Disjoint,
            (Geo::France, Geo::Texas) => SetRelation::Disjoint,

            // ... (omitted other cases and inverses for brevity)
            _ => SetRelation::Trivial,
        }
    }
}

fn main() {
    let mut config = OptimizerConfig {
        merger: GeoMerger,
        merger_depth: 2,
        max_iterations: 0,
    };

    // Example 1. California is inside of USA, so it will be redacted
    {
        let builder = ExpressionBuilder::new();
        let rule = logic!(
            builder,
            any![{ Geo::California }, { Geo::USA }, { Geo::Paris }]
        );
        builder.add_root(rule);

        let mut expr = builder.build();

        let root = expr.roots().next().unwrap();
        println!("1. Before: {}", expr.to_string(root));

        expr.optimize(&mut config);

        let new_root = expr.roots().next().unwrap();
        println!("1. After:  {}", expr.to_string(new_root));
    }
    println!();

    // Example 2. Texas and France are disjoint, so the result is EMPTY
    {
        let builder = ExpressionBuilder::new();
        let rule = logic!(builder, all![{ Geo::Texas }, { Geo::France }]);
        builder.add_root(rule);

        let mut expr = builder.build();

        let root = expr.roots().next().unwrap();
        println!("2. Before: {}", expr.to_string(root));

        expr.optimize(&mut config);

        let new_root = expr.roots().next().unwrap();
        println!("2. After:  {}", expr.to_string(new_root));
    }
    println!();

    // Example 3. California and Texas are both within the USA, so USA is redundant
    {
        let builder = ExpressionBuilder::new();
        let rule = logic!(
            builder,
            all![any![{ Geo::California }, { Geo::Texas }], { Geo::USA }]
        );
        builder.add_root(rule);

        let mut expr = builder.build();

        let root = expr.roots().next().unwrap();
        println!("3. Before: {}", expr.to_string(root));

        expr.optimize(&mut config);

        let new_root = expr.roots().next().unwrap();
        println!("3. After:  {}", expr.to_string(new_root));
    }
}
