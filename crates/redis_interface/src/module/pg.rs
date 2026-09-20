//! Postgres (Supabase) backend for the `redis_interface` crate.
//!
//! It exposes the same public API as the `redis-rs` and `fred` backends
//! (`RedisConnectionPool`, `RedisConnectionWithContext`, `SubscriberClient`,
//! `PublisherClient`, `PubSubMessage`, ...) but never opens a Redis connection:
//! every command is executed against the `pg_kv_cache` / `pg_pubsub_payload`
//! tables. Because the API is unchanged, none of the call sites in `router`,
//! `storage_impl`, `scheduler` or `drainer` need to be touched — selecting the
//! backend is a Cargo feature (`--features postgres`) plus one setting
//! (`redis.postgres_url`).
//!
//! # What is and is not supported
//!
//! Supported (this is everything the router's request path uses): plain keys with
//! TTL, `SETNX`-style conditional writes, `EXPIRE`/`EXPIREAT`/`TTL`, hashes
//! (`HSET`/`HSETNX`/`HGET`/`HGETALL`/`HDEL`/`HINCRBY`/`HSCAN`), sets (`SADD`) and
//! pub/sub (used for cross-instance in-memory-cache invalidation).
//!
//! **Not supported**: Redis *streams* and consumer groups. They back the KV-store
//! drainer and the scheduler's task queue; those binaries (and the per-merchant
//! `RedisKv` storage scheme) need a different design and fail loudly with
//! `RedisError::StreamAppendFailed` & co. rather than pretending to work. The
//! router itself only touches streams when a merchant is on the `RedisKv` scheme.
//!
//! # Pub/sub
//!
//! `publish` appends a row to `pg_pubsub_payload`; every instance polls that table
//! (`POLL_INTERVAL`) and forwards messages on subscribed channels to
//! `SubscriberClient::message_rx()`. Polling — rather than `LISTEN/NOTIFY` — is
//! deliberate: it also works through Supabase's connection poolers and needs no
//! dedicated long-lived connection. Delivery latency is therefore ~`POLL_INTERVAL`,
//! which is fine for cache invalidation. The `pg_cron` sweep removes old rows.

#[path = "redis_rs/types.rs"]
pub mod types;

mod commands;
mod store;

use std::{
    collections::HashSet,
    sync::{atomic, Arc, Mutex, PoisonError},
    time::Duration,
};

use common_utils::{
    errors::CustomResult,
    external_service::{ExternalServiceEventEmitter, NoOpEventEmitter},
    request_context::RequestContext,
};
use error_stack::{report, ResultExt};
use tracing::Instrument;
pub use types::redis_value_to_option_string;

use self::store::PgStore;
use crate::errors::RedisError;

/// How often each instance checks `pg_pubsub_payload` for new messages.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

// ─── Pub/sub ─────────────────────────────────────────────────────────────────

/// Represents a message received from a pub/sub channel.
#[derive(Clone, Debug)]
pub struct PubSubMessage {
    pub channel: String,
    pub value: crate::types::RedisValue,
}

pub struct SubscriberClient {
    channels: Arc<Mutex<HashSet<String>>>,
    broadcast_sender: tokio::sync::broadcast::Sender<PubSubMessage>,
    pub is_subscriber_handler_spawned: Arc<atomic::AtomicBool>,
}

impl SubscriberClient {
    async fn new(
        conf: &crate::types::RedisSettings,
        store: Arc<PgStore>,
    ) -> CustomResult<Self, RedisError> {
        // Only messages published from now on are delivered (Redis semantics).
        let cursor = store
            .max_message_id()
            .await
            .change_context(RedisError::RedisConnectionError)?;

        let (broadcast_sender, _) =
            tokio::sync::broadcast::channel(conf.broadcast_channel_capacity.max(1));
        let channels = Arc::new(Mutex::new(HashSet::new()));

        tokio::spawn(
            Self::run(store, channels.clone(), broadcast_sender.clone(), cursor).in_current_span(),
        );

        Ok(Self {
            channels,
            broadcast_sender,
            is_subscriber_handler_spawned: Arc::new(atomic::AtomicBool::new(false)),
        })
    }

    pub async fn subscribe(&self, channel: &str) -> CustomResult<(), RedisError> {
        self.channels
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(channel.to_owned());
        Ok(())
    }

    pub async fn unsubscribe(&self, channel: &str) -> CustomResult<(), RedisError> {
        self.channels
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(channel);
        Ok(())
    }

    pub fn message_rx(&self) -> tokio::sync::broadcast::Receiver<PubSubMessage> {
        self.broadcast_sender.subscribe()
    }

    /// Polls the pub/sub log and forwards messages on subscribed channels.
    /// The cursor advances over *every* message so messages on channels that
    /// are subscribed later are not replayed.
    async fn run(
        store: Arc<PgStore>,
        channels: Arc<Mutex<HashSet<String>>>,
        broadcast_sender: tokio::sync::broadcast::Sender<PubSubMessage>,
        mut cursor: i64,
    ) {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let records = match store.poll_messages(cursor).await {
                Ok(records) => records,
                Err(error) => {
                    tracing::warn!(?error, "Failed to poll the Postgres pub/sub log");
                    continue;
                }
            };
            for record in records {
                cursor = cursor.max(record.id);
                let subscribed = channels
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .contains(&record.channel);
                if !subscribed {
                    continue;
                }
                // `Err` only means there is no receiver right now; nothing to do.
                let _ = broadcast_sender.send(PubSubMessage {
                    channel: record.channel,
                    value: crate::types::RedisValue::new(redis::Value::BulkString(record.payload)),
                });
            }
        }
    }
}

/// Publishes messages to the pub/sub log.
pub struct PublisherClient {
    store: Arc<PgStore>,
}

impl PublisherClient {
    /// Returns the number of subscribers reached. Postgres cannot tell how many
    /// instances will poll the row, so this reports `1` (the message is stored
    /// and will be delivered to every instance that is subscribed).
    pub async fn publish(
        &self,
        channel: &str,
        message: crate::types::RedisValue,
    ) -> CustomResult<usize, RedisError> {
        let payload = message
            .as_bytes()
            .map(<[u8]>::to_vec)
            .or_else(|| message.as_string().map(String::into_bytes))
            .ok_or_else(|| {
                report!(RedisError::PublishError)
                    .attach_printable("pub/sub message is neither bytes nor a string")
            })?;
        self.store
            .publish(channel, payload)
            .await
            .change_context(RedisError::PublishError)?;
        Ok(1)
    }
}

// ─── Connection pool ─────────────────────────────────────────────────────────

pub struct RedisConnectionPool {
    pub(crate) store: Arc<PgStore>,
    pub key_prefix: String,
    pub config: Arc<RedisConfig>,
    pub subscriber: Arc<SubscriberClient>,
    pub publisher: Arc<PublisherClient>,
    pub is_redis_available: Arc<atomic::AtomicBool>,
    pub event_emitter: Arc<dyn ExternalServiceEventEmitter>,
}

/// A request-scoped handle.
///
/// Wraps the shared [`RedisConnectionPool`] and carries the request ID of the
/// execution that created it, so events can be correlated back to the request.
#[derive(Clone)]
pub struct RedisConnectionWithContext {
    pub redis_conn: Arc<RedisConnectionPool>,
    pub request_id: Option<String>,
}

impl RedisConnectionWithContext {
    pub fn new(pool: Arc<RedisConnectionPool>, context: &dyn RequestContext) -> Self {
        Self {
            redis_conn: pool,
            request_id: context.request_id().map(str::to_owned),
        }
    }

    pub fn new_without_context(pool: Arc<RedisConnectionPool>) -> Self {
        Self {
            redis_conn: pool,
            request_id: None,
        }
    }
}

impl RedisConnectionPool {
    /// Create a new connection pool to the Postgres database
    pub async fn new_without_event_emitter(
        conf: &crate::types::RedisSettings,
    ) -> CustomResult<Self, RedisError> {
        Self::new(conf, Arc::new(NoOpEventEmitter)).await
    }

    pub async fn new(
        conf: &crate::types::RedisSettings,
        event_emitter: Arc<dyn ExternalServiceEventEmitter>,
    ) -> CustomResult<Self, RedisError> {
        if conf.postgres_url.is_empty() {
            return Err(report!(RedisError::InvalidConfiguration(
                "`redis.postgres_url` (env: ROUTER__REDIS__POSTGRES_URL) must be set when the \
                 `postgres` backend is compiled in"
                    .into(),
            )));
        }

        let max_size = u32::try_from(conf.pool_size.max(1)).unwrap_or(u32::MAX);
        let timeout = Duration::from_secs(conf.default_command_timeout.max(1));
        let store = PgStore::connect(conf.postgres_url.expose(), max_size, timeout)
            .await
            .change_context(RedisError::RedisConnectionError)?;
        let store = Arc::new(store);

        let subscriber = Arc::new(SubscriberClient::new(conf, store.clone()).await?);
        let publisher = Arc::new(PublisherClient {
            store: store.clone(),
        });

        Ok(Self {
            store,
            key_prefix: String::new(),
            config: Arc::new(RedisConfig::from(conf)),
            subscriber,
            publisher,
            is_redis_available: Arc::new(atomic::AtomicBool::new(true)),
            event_emitter,
        })
    }

    pub fn clone(&self, key_prefix: &str) -> Self {
        Self {
            store: Arc::clone(&self.store),
            key_prefix: key_prefix.to_string(),
            config: Arc::clone(&self.config),
            subscriber: Arc::clone(&self.subscriber),
            publisher: Arc::clone(&self.publisher),
            is_redis_available: Arc::clone(&self.is_redis_available),
            event_emitter: Arc::clone(&self.event_emitter),
        }
    }

    /// Prefix `key` with this pool's tenant key prefix.
    pub fn add_prefix(&self, key: &str) -> String {
        if self.key_prefix.is_empty() {
            key.to_string()
        } else {
            format!("{}:{}", self.key_prefix, key)
        }
    }

    /// Monitor for connection errors.
    /// When the database is unreachable for longer than `max_failure_threshold_seconds`
    /// seconds, signals via the oneshot sender and marks the store as unavailable.
    pub async fn on_error(&self, tx: tokio::sync::oneshot::Sender<()>) {
        let check_interval = self
            .config
            .unresponsive_check_interval
            .max(crate::constant::redis_rs_commands::MIN_ERROR_CHECK_INTERVAL_SECS);
        let max_unreachable_secs = self.config.max_failure_threshold_seconds;
        let mut first_failure_at: Option<std::time::Instant> = None;

        loop {
            tokio::time::sleep(Duration::from_secs(check_interval)).await;

            let result =
                tokio::time::timeout(Duration::from_secs(check_interval), self.store.ping()).await;

            if matches!(result, Ok(Ok(()))) {
                if first_failure_at.is_some() {
                    tracing::info!("Postgres cache connection restored");
                }
                first_failure_at = None;
            } else {
                let now = std::time::Instant::now();
                let first_failure = *first_failure_at.get_or_insert(now);
                let unreachable_secs = now.duration_since(first_failure).as_secs();

                if unreachable_secs >= u64::from(max_unreachable_secs) {
                    tracing::error!(
                        "Postgres cache has been unreachable for {}s (threshold: {}s), shutting down",
                        unreachable_secs,
                        max_unreachable_secs
                    );
                    if let Err(error) = tx.send(()) {
                        tracing::warn!(
                            ?error,
                            "Failed to send shutdown signal — receiver already dropped"
                        );
                    }
                    self.is_redis_available
                        .store(false, atomic::Ordering::SeqCst);
                    break;
                }

                tracing::warn!(
                    "Postgres cache unreachable for {}s (threshold: {}s), retrying",
                    unreachable_secs,
                    max_unreachable_secs
                );
            }
        }
    }
}

pub struct RedisConfig {
    pub(crate) default_ttl: u32,
    pub(crate) default_hash_ttl: u32,
    pub(crate) unresponsive_check_interval: u64,
    pub(crate) max_failure_threshold_seconds: u32,
}

impl From<&crate::types::RedisSettings> for RedisConfig {
    fn from(config: &crate::types::RedisSettings) -> Self {
        Self {
            default_ttl: config.default_ttl,
            default_hash_ttl: config.default_hash_ttl,
            unresponsive_check_interval: config.unresponsive_check_interval,
            max_failure_threshold_seconds: config.max_failure_threshold_seconds,
        }
    }
}
