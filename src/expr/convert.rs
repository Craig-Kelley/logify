use std::{hash::Hash, mem};

use crate::expr::{Expression, Node};

#[cfg(feature = "fast-binary")]
mod impl_fast_binary {
    use std::hash::Hash;

    use crate::expr::{Expression, ExpressionShadow};
    use bitcode::{Decode, Encode};

    impl<T: Encode> Expression<T> {
        pub fn to_bitcode_bytes(expr: &Expression<T>) -> Vec<u8> {
            bitcode::encode(expr)
        }
    }

    impl<T: Hash + PartialEq + for<'a> Decode<'a>> Expression<T> {
        pub fn from_bitcode_bytes(bytes: &[u8]) -> Result<Self, bitcode::Error> {
            let shadow: ExpressionShadow<T> = bitcode::decode(bytes)?;
            Ok(shadow.into())
        }
    }
}

impl<T> IntoIterator for Expression<T> {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

impl<T: Hash + PartialEq> Extend<Expression<T>> for Expression<T> {
    fn extend<I: IntoIterator<Item = Expression<T>>>(&mut self, iter: I) {
        for mut source in iter {
            if source.nodes.len() == 1 {
                continue;
            }
            let (active, max_root) = source.get_active();
            self.absorb(&active, max_root, &source.roots, |idx| {
                mem::replace(&mut source.nodes[idx], Node::Empty)
            });
        }
    }
}

impl<T> IntoIterator for &Expression<T> {
    type Item = Self;
    type IntoIter = std::iter::Once<Self>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

impl<'a, T: Clone + Hash + PartialEq> Extend<&'a Expression<T>> for Expression<T> {
    fn extend<I: IntoIterator<Item = &'a Expression<T>>>(&mut self, iter: I) {
        for source in iter {
            if source.nodes.len() == 1 {
                continue;
            }
            let (active, max_root) = source.get_active();
            self.absorb(&active, max_root, &source.roots, |idx| {
                source.nodes[idx].clone()
            });
        }
    }
}
