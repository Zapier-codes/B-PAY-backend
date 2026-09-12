//! Logger of the system.

pub use tracing::{debug, error, event as log, info, warn};
pub use tracing_attributes::instrument;

pub mod config;
mod defaults;
pub use crate::config::Config;

// mod macros;
pub mod types;
pub use types::{Category, Flow, Level, Tag};

// `setup` builds the OpenTelemetry/tokio telemetry pipeline (`opentelemetry-otlp`,
// `opentelemetry_sdk`, `tokio`) -- all scoped out of wasm32 builds in this crate's
// Cargo.toml, since mio (pulled by all three) doesn't support wasm32-unknown-unknown.
// Only real server binaries (`router`, `drainer`, `scheduler`) call `setup()`; nothing
// reachable from a wasm32 build does, so the module itself is gated to match.
#[cfg(not(target_arch = "wasm32"))]
mod setup;
#[cfg(not(target_arch = "wasm32"))]
pub use setup::{setup, TelemetryGuard};

pub mod formatter;
pub use formatter::FormattingLayer;

pub mod storage;
pub use storage::{Storage, StorageSubscription};
