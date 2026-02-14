macro_rules! assume {
    ($e:expr $(, $($t:tt)*)?) => {
        let e = $e;
        debug_assert!(e $(, $($t)*)?);
        unsafe { ::core::hint::assert_unchecked(e) }
    };
}

pub(crate) use assume;
