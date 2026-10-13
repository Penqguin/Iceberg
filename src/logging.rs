/// Log a warning — `console_warn!` on wasm32, `eprintln!` on native.
/// Shared with `pipeline`'s validation logging (the client module keeps its
/// own textual copy; this exported version is the crate-wide path).
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "wasm32")]
        worker::console_warn!($($arg)*);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($arg)*);
    }};
}
