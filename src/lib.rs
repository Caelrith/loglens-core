// loglens-core/src/lib.rs

pub mod parsers;
pub mod query;
pub mod time;

// Re-export for easy access
pub use parsers::LogEntry;
pub use query::evaluate;

// Only compile the wasm module if the 'wasm' feature is enabled
#[cfg(feature = "wasm")]
mod wasm;