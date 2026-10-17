//! Debugging helpers from the upstream crate, reduced to a no-op macro.

/// Evaluates to nothing.
macro_rules! debug_print {
    ($($arg:tt)*) => {{}};
}
