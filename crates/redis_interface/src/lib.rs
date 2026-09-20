//! Redis interface — compile-time backend selection via Cargo feature.
//!
//! By default the `redis-rs` crate is used. Enable the `fred` feature to switch, or
//! the `postgres` feature to run the very same API on top of Postgres (Supabase)
//! with no Redis server at all.
//!
//! # Examples
//! ```ignore
//! use std::sync::Arc;
//! use common_utils::external_service::NoOpEventEmitter;
//! use redis_interface::{types::RedisSettings, RedisConnectionPool};
//!
//! #[tokio::main]
//! async fn main() {
//!     let emitter = Arc::new(NoOpEventEmitter);
//!     let redis_conn = RedisConnectionPool::new(&RedisSettings::default(), emitter).await;
//! }
//! ```

// Compile-time guards: exactly one backend must be active.
#[cfg(not(any(feature = "fred", feature = "redis-rs", feature = "postgres")))]
compile_error!(
    "One of the features \"fred\", \"redis-rs\" or \"postgres\" must be enabled for this crate."
);

#[cfg(any(
    all(feature = "fred", feature = "redis-rs"),
    all(feature = "fred", feature = "postgres"),
    all(feature = "redis-rs", feature = "postgres"),
))]
compile_error!(
    "Features \"fred\", \"redis-rs\" and \"postgres\" are mutually exclusive — enable only one."
);

pub mod constant;
pub mod errors;
// Per-roundtrip Redis metrics/events; the Postgres backend has no Redis roundtrips.
#[cfg(any(feature = "fred", feature = "redis-rs"))]
pub(crate) mod metrics;
pub mod types;

#[cfg(feature = "fred")]
mod module {
    pub mod fred;
}

#[cfg(feature = "redis-rs")]
mod module {
    pub mod redis_rs;
}

#[cfg(feature = "postgres")]
mod module {
    pub mod pg;
}

// Re-export the active backend's public types under unified names.
// All external code imports `redis_interface::RedisConnectionPool` etc.
// and is never aware of which backend is active.

#[cfg(feature = "fred")]
pub use fred::interfaces::{EventInterface, PubsubInterface};
#[cfg(feature = "fred")]
pub use module::fred::{
    PubSubMessage, RedisClient, RedisConfig, RedisConnectionPool, RedisConnectionWithContext,
    SubscriberClient,
};
#[cfg(feature = "postgres")]
pub use module::pg::{
    redis_value_to_option_string, PubSubMessage, PublisherClient, RedisConfig, RedisConnectionPool,
    RedisConnectionWithContext, SubscriberClient,
};
#[cfg(feature = "redis-rs")]
pub use module::redis_rs::{
    redis_value_to_option_string, PubSubMessage, PublisherClient, RedisConfig, RedisConn,
    RedisConnectionPool, RedisConnectionWithContext, SubscriberClient,
};

pub use self::types::*;

// The Redis test-suite talks to a live Redis server and exercises commands the
// Postgres backend deliberately does not offer.
#[cfg(all(test, not(feature = "postgres")))]
mod test;

// Postgres-backend tests; they need a database, see the module docs.
#[cfg(all(test, feature = "postgres"))]
mod test_pg;
