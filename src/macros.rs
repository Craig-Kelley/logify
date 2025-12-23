#[macro_export]
#[doc(hidden)]
macro_rules! logic_list {
    ($b:ident, $($input:tt)*) => {
        $crate::logic_list!(@recurse $b, [$($input)*] -> [])
    };

	// base
    (@recurse $b:ident, [] -> [$($out:expr),*]) => {
        vec![ $($out),* ]
    };

	// munchers
    (@recurse $b:ident, [ $k:ident ! [ $($args:tt)* ] , $($rest:tt)* ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [$($rest)*] -> [
            $($out,)*
            $crate::logic!($b, $k ! [ $($args)* ])
        ])
    };
    (@recurse $b:ident, [ $k:ident ! [ $($args:tt)* ] ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [] -> [
            $($out,)*
            $crate::logic!($b, $k ! [ $($args)* ])
        ])
    };

    // !
    (@recurse $b:ident, [ ! $val:tt , $($rest:tt)* ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [$($rest)*] -> [
            $($out,)*
            $crate::logic!($b, ! $val)
        ])
    };
    (@recurse $b:ident, [ ! $val:tt ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [] -> [
            $($out,)*
            $crate::logic!($b, ! $val)
        ])
    };

    // other
    (@recurse $b:ident, [ $val:tt , $($rest:tt)* ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [$($rest)*] -> [
            $($out,)*
            $crate::logic!($b, $val)
        ])
    };
    (@recurse $b:ident, [ $val:tt ] -> [$($out:expr),*]) => {
        $crate::logic_list!(@recurse $b, [] -> [
            $($out,)*
            $crate::logic!($b, $val)
        ])
    };
}

#[macro_export]
macro_rules! logic {
    ($builder:ident, $($input:tt)+) => {
        $crate::logic!(@recurse $builder, [ $($input)* ] -> [])
    };

    // exit
    (@recurse $b:ident, [] -> [$($out:tt)*]) => { $($out)* };

	// any![]
    (@recurse $b:ident, [ any ! [ $($args:tt)* ] $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [
            $($out)*
            {
                let safe_b = $crate::builder::ExpressionBuilder::__check_type(&$b);
                safe_b.wrap(safe_b.union( $crate::logic_list!($b, $($args)*) ))
            }
        ])
    };

	// all![]
	(@recurse $b:ident, [ all ! [ $($args:tt)* ] $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [
            $($out)*
            {
                let safe_b = $crate::builder::ExpressionBuilder::__check_type(&$b);
                safe_b.wrap(safe_b.intersection( $crate::logic_list!($b, $($args)*) ))
            }
        ])
    };

	// |
    (@recurse $b:ident, [ | $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [$($out)* |])
    };
	// &
    (@recurse $b:ident, [ & $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [$($out)* &])
    };
	// // ^
    // (@recurse $b:ident, [ ^ $($rest:tt)* ] -> [$($out:tt)*]) => {
    //     $crate::logic!(@recurse $b, [$($rest)*] -> [$($out)* ^])
    // };
	// !
    (@recurse $b:ident, [ ! $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [$($out)* !])
    };

    // groups
    (@recurse $b:ident, [ ( $($inner:tt)* ) $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [
            $($out)*
            ( $crate::logic!($b, $($inner)*) )
        ])
    };

    // leaves
    (@recurse $b:ident, [ $val:tt $($rest:tt)* ] -> [$($out:tt)*]) => {
        $crate::logic!(@recurse $b, [$($rest)*] -> [
            $($out)*
            {
                let safe_b = $crate::builder::ExpressionBuilder::__check_type(&$b);
                safe_b.leaf($val)
            }
        ])
    };
}
