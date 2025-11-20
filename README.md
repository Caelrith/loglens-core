# LogLens Core 🔍

[![Crates.io](https://img.shields.io/crates/v/loglens-core.svg)](https://crates.io/crates/loglens-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**The lightning-fast, structured log parsing engine built in Rust.**

`loglens-core` is a high-performance library designed to parse, query, and analyze structured logs (JSON, Nginx, Logfmt) at scale. It powers the [LogLens CLI](https://getloglens.com).

## 🚀 Features

* **Zero-Config Parsing:** Automatically detects and parses JSON or Logfmt lines without manual schema definition.
* **Structured Query Engine:** Filter logs with complex logic using a familiar syntax: `level == "error" && latency > 500`.
* **High Throughput:** Optimized for reading gigabytes of logs in seconds, making it suitable for high-volume production environments.
* **Time-Aware:** Native understanding of timestamps for time-range filtering.

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
loglens-core = "0.1.0"
```

## ⚡ Usage Example

Here is how you can use `loglens-core` to parse a log line and run a structured query against it:

```rust
use loglens_core::{evaluate, LogEntry, parsers::parse_log_line};

fn main() {
    // A sample JSON log line
    let log_line = r#"{"level": "error", "latency": 502, "msg": "Database timeout", "timestamp": "2023-10-27T10:00:00Z"}"#;

    // 1. Parse the raw string into a structured LogEntry
    // This automatically detects if it is JSON or Logfmt
    let entry = parse_log_line(log_line);

    // 2. Define a query
    // Returns true if latency is greater than 500 AND level is "error"
    let query = "latency > 500 && level is error";
    
    // 3. Evaluate the query against the parsed entry
    if let LogEntry::Structured(value) = entry {
        // 'evaluate' returns a Result<bool, QueryError>
        match evaluate(&value, log_line, query) {
            Ok(true) => println!("✅ Match found!"),
            Ok(false) => println!("❌ No match."),
            Err(e) => eprintln!("Query error: {}", e),
        }
    } else {
        println!("Could not parse as structured log.");
    }
}
```

## 🖥️ Want the full CLI experience?

This library is the engine behind **LogLens**, a terminal-based log explorer that replaces `jq` and `tail -f` with a powerful TUI.

![LogLens TUI](https://github.com/user-attachments/assets/7d8244cc-1c49-46f8-9454-df26ba209a6f)

**LogLens Pro** features:
* 📊 **Interactive TUI:** Filter and explore logs with keyboard shortcuts.
* 👀 **Live Watch:** Real-time filtering and highlighting (`loglens watch`).
* 📈 **Instant Stats:** Automatic field analysis and error rate calculation.

**[Install LogLens via Homebrew](https://getloglens.com)**
```bash
brew tap Caelrith/loglens
brew install loglens
```

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📄 License

This project is licensed under the [MIT License](LICENSE).