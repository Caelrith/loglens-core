pub mod parsers;
pub mod query;
pub mod time;

// Re-export for easy access
pub use parsers::LogEntry;
pub use query::evaluate;