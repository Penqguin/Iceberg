/// Log a warning — `console_warn!` on wasm32, `eprintln!` on native.
/// The crate's single definition: modules that need to warn do
/// `use crate::log_warn;` and call the macro unqualified.
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {{
        #[cfg(target_arch = "wasm32")]
        worker::console_warn!($($arg)*);
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($arg)*);
    }};
}
