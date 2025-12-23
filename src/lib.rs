#[macro_use]
mod macros;

pub mod builder;
pub mod eval;
pub mod expr;
pub mod opt;

pub mod prelude {
    pub use crate::builder::ExpressionBuilder;
    pub use crate::eval::{Evaluator, EvaluatorCache};
    pub use crate::expr::{Expression, Node, NodeId};
    pub use crate::opt::{MergeResult, Mergeable, OptimizerConfig, SetRelation};

    pub use crate::logic;
}

pub use builder::ExpressionBuilder;
pub use eval::{Evaluator, EvaluatorCache};
pub use expr::{Expression, NodeId};
