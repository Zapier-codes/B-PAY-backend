//! Postgres-backed replacement for `redis/cache.rs` + `redis/kv_store.rs`
//! (Task 73/a).
//!
//! Matches the subset of `RedisStore`'s public shape that the 63 real
//! Redis call sites under `crates/router/src` actually use (per Task 73/a's
//! categorization: 29 caching, 39 locking; a file can be counted in both).
//! Pub/sub (the 18-file third category) is `pg_pub_sub.rs`, not this file.
//!
//! ## What is and isn't done here
//! Implemented: atomic get/set/setnx for plain keys, atomic
//! hset/hget/hsetnx/hscan for hash-field keys, TTL applied the same way the
//! KV wrapper does today (one shared `ttl_for_kv`, not a per-field TTL) —
//! see `up.sql` for why this is one-row-per-field rather than a JSON blob.
//!
//! Not done, left for the per-call-site review Task 73/a explicitly calls
//! out as separate follow-up work:
//! - **Expiry sweep — fixed this session, via `pg_cron`, not the
//!   `scheduler` crate.** `pg_kv_cache.expires_at` is still filtered at
//!   read time (matches Redis's own lazy-expiry read behaviour), but rows
//!   past `expires_at` are now also reclaimed by a scheduled
//!   `sweep_pg_kv_cache()` DB function (bounded-batch delete, every 5
//!   minutes) — see
//!   `migrations/2026-09-11-130000_add_pg_kv_cache_pubsub_sweep_jobs/up.sql`.
//!   A `scheduler`-crate job was the option floated when this gap was
//!   first flagged, but pg_cron needs no Rust-side wiring or a second
//!   process to deploy/monitor, and Task 73's own scope decision already
//!   named pg_cron as the mechanism for the Kafka/ClickHouse replacement —
//!   using it here too avoids introducing a second scheduling mechanism
//!   for the same kind of problem.
//! - **`KvOperation::Scan`'s LIKE-metacharacter escaping — fixed this
//!   session** (was flagged here as open). `scan_hash_fields`'s glob-to-
//!   `LIKE` translation now escapes `%`/`_`/`\` before turning `*` into an
//!   unescaped `%`, paired with `LIKE ... ESCAPE '\'` in the query — see
//!   `glob_to_escaped_sql_like`'s own doc comment for what's still
//!   deliberately not translated (`?`, char classes) and why.
//! - `KvOperation::Scan`'s Redis implementation (`hscan_and_deserialize`)
//!   uses Redis's `HSCAN` cursor semantics for large hashes; this version
//!   does a single indexed `SELECT ... WHERE cache_key = $1 AND field LIKE
//!   $2`, correct for the data volumes seen in the 29 real caching call
//!   sites audited so far but not cursor-paginated — flag if a call site
//!   turns out to scan an unbounded hash.
//! - No load testing against Supabase. Latency profile will differ
//!   meaningfully from Redis (network round trip to Postgres per op instead
//!   of an in-memory store) — real for high-QPS call sites, not assumed
//!   away here.
//! - Per-call-site TTL/atomicity audit (Task 73/a's own explicit next step
//!   for all 63 files) has not started.
//! - This module has not been compiled against the real workspace yet —
//!   a real attempt was made this session (this sandbox can reach
//!   `crates.io`), and hit a diagnosed blocker: the workspace pins
//!   `rust-version = "1.85.0"`, the only toolchain this sandbox's package
//!   sources offer is `1.75.0`, and there's no network path here to a
//!   newer one. Treat as a reviewed-by-reading, partially-hardened first
//!   draft, not a proven one.
//!
//! Do not treat this as a drop-in production replacement without both a
//! real build/test pass and the per-call-site audit above.

use async_bb8_diesel::AsyncRunQueryDsl;
use diesel::{sql_query, sql_types::Text, QueryableByName};
use error_stack::{report, ResultExt};
use serde::{de::DeserializeOwned, Serialize};
use time::PrimitiveDateTime;

use crate::errors::StorageError;

/// Deliberately its own pool type, distinct from
/// `crate::database::store::PgPool` (which wraps `RawPgPool` with an event
/// emitter for the domain-model store). This module is a narrower,
/// self-contained choke-point and doesn't need that wrapper — but wiring it
/// into the real connection-startup path (`database/store.rs`) so it shares
/// one physical pool instead of opening a second one is unresolved, left
/// for integration, not solved here.
pub type PgKvPool = bb8::Pool<async_bb8_diesel::ConnectionManager<diesel::PgConnection>>;

/// Mirrors `redis_interface::SetnxReply` so callers that only branch on
/// "did my write win the race" don't need to know which backend answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgSetnxReply {
    KeySet,
    KeyNotSet,
}

/// Mirrors `redis_interface::HsetnxReply`, same reasoning as `PgSetnxReply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PgHsetnxReply {
    KeySet,
    KeyNotSet,
}

#[derive(QueryableByName)]
struct RawValueRow {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    value: Vec<u8>,
}

#[derive(QueryableByName)]
struct InsertedRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    #[allow(dead_code)] // existence of the row is the signal; id itself unused today
    id: i64,
}

pub struct PgKvStore {
    pool: PgKvPool,
    /// Same role as `KVRouterStore::ttl_for_kv` — one shared TTL applied to
    /// every write through this store, not a per-call override. Matching
    /// the existing convention rather than inventing a new one, per Task
    /// 73/a's "matching RedisStore's existing public shape" framing.
    ttl_seconds: i64,
}

impl PgKvStore {
    pub fn new(pool: PgKvPool, ttl_seconds: i64) -> Self {
        Self { pool, ttl_seconds }
    }

    /// Matches the rest of this crate's convention (`common_utils::date_time::now()`
    /// → `time::PrimitiveDateTime`, e.g. `merchant_connector_account.rs`), not a
    /// separately-invented timestamp type.
    fn expiry_from_now(&self) -> PrimitiveDateTime {
        common_utils::date_time::now() + time::Duration::seconds(self.ttl_seconds)
    }

    // ---- plain key/value (field = '') ----------------------------------

    /// Analog of `serialize_and_set_key_if_not_exist`. Atomic via
    /// `INSERT ... ON CONFLICT DO NOTHING RETURNING id` — a single
    /// round-trip, no read-then-write race window.
    pub async fn set_key_if_not_exist<S: Serialize + Sync>(
        &self,
        key: &str,
        value: &S,
    ) -> error_stack::Result<PgSetnxReply, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let bytes =
            serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?;
        let expires_at = self.expiry_from_now();

        let rows: Vec<InsertedRow> = sql_query(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, '', $2, $3) \
             ON CONFLICT (cache_key, field) DO NOTHING \
             RETURNING id",
        )
        .bind::<Text, _>(key)
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .bind::<diesel::sql_types::Timestamp, _>(expires_at)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(if rows.is_empty() {
            PgSetnxReply::KeyNotSet
        } else {
            PgSetnxReply::KeySet
        })
    }

    /// Analog of `get_and_deserialize_key`. Applies the same not-expired
    /// filter Redis's own TTL gives for free — see `up.sql`'s design note
    /// on lazy expiry.
    pub async fn get_key<T: DeserializeOwned>(
        &self,
        key: &str,
    ) -> error_stack::Result<T, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        let rows: Vec<RawValueRow> = sql_query(
            "SELECT value FROM pg_kv_cache \
             WHERE cache_key = $1 AND field = '' \
             AND (expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))",
        )
        .bind::<Text, _>(key)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| report!(StorageError::ValueNotFound(format!("pg cache key not found: {key}"))))?;
        serde_json::from_slice(&row.value).change_context(StorageError::DeserializationFailed)
    }

    // ---- hash-field key/value (HSET/HGET/HSETNX/HSCAN analogs) ---------

    /// Analog of `set_hash_fields`. Upserts, same as Redis `HSET` (it does
    /// not require the field to be absent — that's `HSETNX`'s job below).
    pub async fn set_hash_field<S: Serialize + Sync>(
        &self,
        key: &str,
        field: &str,
        value: &S,
    ) -> error_stack::Result<(), StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let bytes =
            serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?;
        let expires_at = self.expiry_from_now();

        sql_query(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (cache_key, field) \
             DO UPDATE SET value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, \
                            updated_at = now()",
        )
        .bind::<Text, _>(key)
        .bind::<Text, _>(field)
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .bind::<diesel::sql_types::Timestamp, _>(expires_at)
        .execute_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(())
    }

    /// Analog of `get_hash_field_and_deserialize`.
    pub async fn get_hash_field<T: DeserializeOwned>(
        &self,
        key: &str,
        field: &str,
    ) -> error_stack::Result<T, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        let rows: Vec<RawValueRow> = sql_query(
            "SELECT value FROM pg_kv_cache \
             WHERE cache_key = $1 AND field = $2 \
             AND (expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))",
        )
        .bind::<Text, _>(key)
        .bind::<Text, _>(field)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        let row = rows
            .into_iter()
            .next()
            .ok_or_else(|| report!(StorageError::ValueNotFound(format!("pg cache field not found: {key}.{field}"))))?;
        serde_json::from_slice(&row.value).change_context(StorageError::DeserializationFailed)
    }

    /// Analog of `serialize_and_set_hash_field_if_not_exist`. Same
    /// single-round-trip atomicity approach as `set_key_if_not_exist`.
    pub async fn set_hash_field_if_not_exist<S: Serialize + Sync>(
        &self,
        key: &str,
        field: &str,
        value: &S,
    ) -> error_stack::Result<PgHsetnxReply, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let bytes =
            serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?;
        let expires_at = self.expiry_from_now();

        let rows: Vec<InsertedRow> = sql_query(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (cache_key, field) DO NOTHING \
             RETURNING id",
        )
        .bind::<Text, _>(key)
        .bind::<Text, _>(field)
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .bind::<diesel::sql_types::Timestamp, _>(expires_at)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(if rows.is_empty() {
            PgHsetnxReply::KeyNotSet
        } else {
            PgHsetnxReply::KeySet
        })
    }

    /// Analog of `hscan_and_deserialize`. Not cursor-paginated — see the
    /// module doc comment's "not done" list before pointing this at an
    /// unbounded hash.
    pub async fn scan_hash_fields<T: DeserializeOwned>(
        &self,
        key: &str,
        field_pattern: &str,
    ) -> error_stack::Result<Vec<T>, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let sql_pattern = glob_to_escaped_sql_like(field_pattern);

        let rows: Vec<RawValueRow> = sql_query(
            "SELECT value FROM pg_kv_cache \
             WHERE cache_key = $1 AND field LIKE $2 ESCAPE '\\' \
             AND (expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))",
        )
        .bind::<Text, _>(key)
        .bind::<Text, _>(sql_pattern)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        if rows.is_empty() {
            return Err(report!(StorageError::ValueNotFound(format!(
                "pg cache scan found no fields for key: {key}"
            ))));
        }

        rows.into_iter()
            .map(|row| {
                serde_json::from_slice(&row.value)
                    .change_context(StorageError::DeserializationFailed)
            })
            .collect()
    }
}

/// Redis SCAN globs (`*`) map onto SQL `LIKE` (`%`). **Fixed this session
/// (was the module doc comment's flagged "not done" gap):** LIKE's own
/// metacharacters (`%`, `_`) and its escape character (`\`) are escaped
/// *before* `*` is translated, and the query pairs this with
/// `LIKE ... ESCAPE '\'` — so a literal `%`, `_`, or `\` in a real field
/// name now matches itself instead of being misread as a SQL wildcard.
/// Redis SCAN globs also support `?` (single-char wildcard) and
/// `[abc]`/`[a-z]` (char classes); neither is translated here — left as a
/// literal character like any other, not guessed at, since none of the 63
/// call sites were confirmed to rely on them. If a future per-call-site
/// audit finds one that does, extend this function then, with that call
/// site's actual pattern in hand, rather than speculatively now.
fn glob_to_escaped_sql_like(field_pattern: &str) -> String {
    let mut escaped = String::with_capacity(field_pattern.len());
    for ch in field_pattern.chars() {
        match ch {
            '*' => escaped.push('%'),
            '%' | '_' | '\\' => {
                escaped.push('\\');
                escaped.push(ch);
            }
            other => escaped.push(other),
        }
    }
    escaped
}
