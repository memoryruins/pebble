#![no_std]
#![feature(const_generics, decl_macro)]

mod binary_pretty_print;
pub mod bitmap;
pub mod math;

pub use self::binary_pretty_print::BinaryPrettyPrint;

/// This macro should be called at the beginning of functions that create logic errors if they are
/// called more than once. Most commonly this is used for initialization functions.
pub macro assert_first_call
{
    () =>
    {
        assert_first_call!("ASSERTION FAILED: function has already been called");
    },

    ($($arg:tt)+) =>
    {{
        fn assert_first_call()
        {
            use core::sync::atomic::{AtomicBool,
                                     ATOMIC_BOOL_INIT,
                                     Ordering};

            static CALLED : AtomicBool = ATOMIC_BOOL_INIT;
            let called = CALLED.swap(true, Ordering::Relaxed);
            assert!(!called, $($arg)+);
        }
        assert_first_call();
    }}
}
