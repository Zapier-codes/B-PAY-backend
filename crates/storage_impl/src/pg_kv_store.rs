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
//! - **Findings #6/#7/#8/#9 — fixed this session, via new methods, not by
//!   changing the four methods above.** The per-call-site audit's third
//!   through fifth passes found this module's original shape (setnx-only
//!   writes, no batch hash write, no TTL introspection, no delete at all)
//!   didn't cover every real Redis call site: `set_key_with_expiry` +
//!   `update_key_preserving_ttl` (Finding #6, the single highest-call-
//!   count gap — unconditional overwrite-with-TTL and its TTL-preserving
//!   counterpart, for `db/payment_method_session.rs` and three
//!   `core/payment_methods.rs` sites), `set_hash_fields` (Finding #7, one
//!   round-trip multi-field write so `core/payment_method_balance.rs`
//!   can't observe a partial write), `get_ttl` (Finding #8, for
//!   `vault.rs`'s CVC-retrieval path), and `delete_key`/`delete_hash_field`
//!   (Finding #9 — real revocation, not lazy-expiry-eligible housekeeping,
//!   for single-use payment tokens and temp-locker cleanup). Not yet
//!   wired into any real call site — that's still the unstarted
//!   `RedisStore` integration this file's own second bullet below already
//!   flags — and not yet compiled, same toolchain wall as everything else
//!   here.
//!
//! - **Finding #10 — fixed this session (sixth audit pass), same as
//!   #6–#9: a new method, no existing method changed.** `exists`, for
//!   `services/authentication/blacklist.rs` and `utils/user/two_factor_
//!   auth.rs`'s plain `EXISTS` checks — distinct from `get_key` (which
//!   also deserializes a value these callers discard) and `get_ttl`
//!   (Finding #8, which needs a remaining-duration answer, not a bool).
//!   Not wired into any real call site, not compiled — same caveats as
//!   Findings #6–#9 above.
//!
//! - **Finding #11 — fixed this session (eighth audit pass), same
//!   additive pattern as #6–#10.** `set_key_if_not_exist_with_expiry`,
//!   for `utils/currency.rs`'s forex-refresh distributed lock, which
//!   passes its own lock-timeout duration rather than this store's
//!   default `ttl_seconds` — a real per-call TTL override
//!   `set_key_if_not_exist` had no way to accept. Not wired into any
//!   real call site, not compiled — same caveats as every other finding
//!   above.
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

    /// Analog of `set_key_if_not_exists_with_expiry` — same single-round-
    /// trip SETNX atomicity as `set_key_if_not_exist` above, but with an
    /// explicit per-call TTL override instead of this store's own
    /// `ttl_seconds`. Found in the per-call-site audit's eighth pass:
    /// `utils/currency.rs`'s `acquire_redis_lock` (a distributed lock
    /// guarding a refresh of the forex-rate cache) passes its own
    /// configured lock-timeout duration, not the store's default cache
    /// TTL. Reusing `set_key_if_not_exist`'s fixed `ttl_seconds` here
    /// would silently hold the lock for the wrong duration — wrong in
    /// either direction: too short risks two callers racing the refresh,
    /// too long risks a crashed holder blocking every other caller until
    /// the store's unrelated default cache TTL happens to lapse. `None`
    /// falls back to this store's own `ttl_seconds`, matching
    /// `set_key_if_not_exist`'s existing behaviour for callers that don't
    /// need an override.
    pub async fn set_key_if_not_exist_with_expiry<S: Serialize + Sync>(
        &self,
        key: &str,
        value: &S,
        expiry_seconds: Option<i64>,
    ) -> error_stack::Result<PgSetnxReply, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let bytes =
            serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?;
        let expires_at = match expiry_seconds {
            Some(secs) => common_utils::date_time::now() + time::Duration::seconds(secs),
            None => self.expiry_from_now(),
        };

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

    // ---- overwrite-with-TTL variants (Finding #6) -----------------------
    //
    // `set_key_if_not_exist` above is Redis SETNX semantics: a no-op if the
    // key already exists. The per-call-site audit (third/fourth/fifth
    // passes) found the real gap is Redis's *other* common shape —
    // `serialize_and_set_key_with_expiry`, an unconditional overwrite that
    // also resets the TTL — needed by `db/payment_method_session.rs` and
    // three sites in `core/payment_methods.rs` (single-use payment-method
    // tokens, volatile payment-method records, CVC tokens). This was the
    // single highest-call-count gap in this module; reusing SETNX for
    // these call sites would silently keep serving a stale value forever
    // after the first write, a correctness bug not a style choice.

    /// Analog of `serialize_and_set_key_with_expiry`. Unconditionally
    /// overwrites the value *and* resets the TTL to a fresh
    /// `ttl_seconds`-from-now window, in one round trip.
    pub async fn set_key_with_expiry<S: Serialize + Sync>(
        &self,
        key: &str,
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
             VALUES ($1, '', $2, $3) \
             ON CONFLICT (cache_key, field) \
             DO UPDATE SET value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, \
                            updated_at = now()",
        )
        .bind::<Text, _>(key)
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .bind::<diesel::sql_types::Timestamp, _>(expires_at)
        .execute_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(())
    }

    /// TTL-*preserving* counterpart to `set_key_with_expiry`, for the
    /// session-expiry security boundary in `db/payment_method_session.rs`
    /// (third pass): overwrites the value without touching `expires_at`.
    /// On first write for a key that doesn't exist yet there is no
    /// existing TTL to preserve, so this falls back to a fresh
    /// `ttl_seconds` window rather than silently writing a row with no
    /// expiry at all.
    pub async fn update_key_preserving_ttl<S: Serialize + Sync>(
        &self,
        key: &str,
        value: &S,
    ) -> error_stack::Result<(), StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let bytes =
            serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?;
        let fallback_expires_at = self.expiry_from_now();

        sql_query(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, '', $2, $3) \
             ON CONFLICT (cache_key, field) \
             DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind::<Text, _>(key)
        .bind::<diesel::sql_types::Binary, _>(bytes)
        .bind::<diesel::sql_types::Timestamp, _>(fallback_expires_at)
        .execute_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(())
    }

    // ---- TTL introspection (Finding #8) ---------------------------------

    /// Analog of `vault.rs`'s `redis_conn.get_ttl(&key)` — returns the
    /// *remaining* time-to-live, not the value. Used by
    /// `retrieve_key_and_ttl_for_cvc_from_payment_method_id` to compute an
    /// expiry timestamp shown back to the caller. Returns `Ok(None)` for a
    /// plain (non-expiring) row, matching Redis `TTL`'s `-1` case, and a
    /// `ValueNotFound` error for a missing/already-expired key, matching
    /// Redis `TTL`'s `-2` case.
    pub async fn get_ttl(
        &self,
        key: &str,
    ) -> error_stack::Result<Option<time::Duration>, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        #[derive(QueryableByName)]
        struct TtlRow {
            #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Double>)]
            remaining_seconds: Option<f64>,
        }

        let rows: Vec<TtlRow> = sql_query(
            "SELECT extract(epoch FROM (expires_at - (now() AT TIME ZONE 'utc'))) AS remaining_seconds \
             FROM pg_kv_cache \
             WHERE cache_key = $1 AND field = '' \
             AND (expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))",
        )
        .bind::<Text, _>(key)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        let row = rows.into_iter().next().ok_or_else(|| {
            report!(StorageError::ValueNotFound(format!(
                "pg cache key not found or expired: {key}"
            )))
        })?;

        Ok(row
            .remaining_seconds
            .map(|secs| time::Duration::seconds_f64(secs.max(0.0))))
    }

    // ---- deletion / revocation (Finding #9) -----------------------------
    //
    // Everything above relies on lazy expiry (a row past `expires_at` is
    // filtered at read time, reclaimed later by the `pg_cron` sweep) —
    // fine for cache housekeeping, wrong for *revocation*. Single-use
    // payment tokens (`core/payment_methods/utils.rs`'s
    // `delete_payment_token_data`) and temp-locker cleanup (`vault.rs`)
    // both need a token to stop being valid the instant it's deleted, not
    // whenever its original TTL happens to lapse. Falling back to "do
    // nothing, let the row expire on its own" would be a real security
    // regression, not a missing convenience.

    /// Analog of Redis `DEL` for a plain key. Idempotent: deleting an
    /// already-absent key is not an error, matching Redis's own `DEL`
    /// (returns `0`, not an error, for a missing key).
    pub async fn delete_key(&self, key: &str) -> error_stack::Result<(), StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        sql_query("DELETE FROM pg_kv_cache WHERE cache_key = $1 AND field = ''")
            .bind::<Text, _>(key)
            .execute_async(&conn)
            .await
            .map_err(StorageError::from)?;

        Ok(())
    }

    /// Analog of Redis `HDEL` for a single hash field.
    pub async fn delete_hash_field(
        &self,
        key: &str,
        field: &str,
    ) -> error_stack::Result<(), StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        sql_query("DELETE FROM pg_kv_cache WHERE cache_key = $1 AND field = $2")
            .bind::<Text, _>(key)
            .bind::<Text, _>(field)
            .execute_async(&conn)
            .await
            .map_err(StorageError::from)?;

        Ok(())
    }

    // ---- batch hash write (Finding #7) ----------------------------------

    /// Analog of `set_hash_fields` (plural) — `core/payment_method_
    /// balance.rs`'s `persist_individual_pm_balance_details_in_redis`
    /// writes a whole balance record's fields in one Redis call so a
    /// concurrent reader never observes a partial write. A loop over the
    /// singular `set_hash_field` above would not have that guarantee: this
    /// uses one `INSERT ... SELECT ... FROM UNNEST(...)` round trip
    /// instead, so every field in `fields` lands in the same statement —
    /// same atomicity Redis gives a multi-field `HSET` for free.
    pub async fn set_hash_fields<S: Serialize + Sync>(
        &self,
        key: &str,
        fields: &[(String, S)],
    ) -> error_stack::Result<(), StorageError> {
        if fields.is_empty() {
            return Ok(());
        }
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        let mut field_names = Vec::with_capacity(fields.len());
        let mut field_values = Vec::with_capacity(fields.len());
        for (field, value) in fields {
            field_names.push(field.clone());
            field_values.push(
                serde_json::to_vec(value).change_context(StorageError::SerializationFailed)?,
            );
        }
        let expires_at = self.expiry_from_now();

        sql_query(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             SELECT $1, f, v, $4 \
             FROM UNNEST($2::text[], $3::bytea[]) AS t(f, v) \
             ON CONFLICT (cache_key, field) \
             DO UPDATE SET value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, \
                            updated_at = now()",
        )
        .bind::<Text, _>(key)
        .bind::<diesel::sql_types::Array<Text>, _>(field_names)
        .bind::<diesel::sql_types::Array<diesel::sql_types::Binary>, _>(field_values)
        .bind::<diesel::sql_types::Timestamp, _>(expires_at)
        .execute_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(())
    }
    // ---- key existence (Finding #10) ------------------------------------
    //
    // Found in the per-call-site audit's sixth pass, not the original four
    // findings: `services/authentication/blacklist.rs`'s
    // `check_email_token_in_blacklist` and `utils/user/two_factor_auth.rs`'s
    // `check_totp_in_redis` / `check_recovery_code_in_redis` all call
    // `redis_conn.exists::<()>(...)` — a plain existence check, distinct
    // from `get_key` (which also deserializes a value the caller doesn't
    // want here) and from `get_ttl` (Finding #8, which needs a live,
    // non-expired row to compute a remaining duration from). No `pg_kv_
    // store.rs` method covers "does this key exist, expired-filtered,
    // value discarded" today.

    /// Analog of Redis `EXISTS` for a plain key. Applies the same
    /// not-expired filter every read in this module already applies —
    /// an expired-but-not-yet-swept row reads as absent, matching Redis's
    /// own lazy-expiry behaviour (a TTL'd key that's past its expiry
    /// answers `EXISTS` with `0`, same as a key that was never set).
    pub async fn exists(&self, key: &str) -> error_stack::Result<bool, StorageError> {
        let conn = self
            .pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        #[derive(QueryableByName)]
        struct ExistsRow {
            #[diesel(sql_type = diesel::sql_types::Bool)]
            #[allow(dead_code)] // existence of the row is the signal; value itself unused
            present: bool,
        }

        let rows: Vec<ExistsRow> = sql_query(
            "SELECT true AS present FROM pg_kv_cache \
             WHERE cache_key = $1 AND field = '' \
             AND (expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))",
        )
        .bind::<Text, _>(key)
        .load_async(&conn)
        .await
        .map_err(StorageError::from)?;

        Ok(!rows.is_empty())
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
