#![allow(unused)]

macro_rules! assume {
    ($e:expr $(, $($t:tt)*)?) => {
        let e = $e;
        debug_assert!(e $(, $($t)*)?);
        unsafe { ::core::hint::assert_unchecked(e) }
    };
}

macro_rules! goto {
    ($curr:expr, $label:lifetime: $next:expr, $($t:tt)+) => {
        goto!({ $label: { $curr } $next }, $($t)+)
    };
    ($curr:expr, $label:lifetime: $next:expr $(,)?) => {
        $label: { $curr } $next
    };
}

pub(crate) use {assume, goto};
