//! Postgres advisory-lock replacement for the "locking" category of Task
//! 73/a's 63 real Redis call sites (39 of the 63; a file can also be
//! counted under "caching").
//!
//! ## Why session-level advisory locks, and the correctness trap they carry
//! Postgres has two advisory-lock flavors: transaction-scoped
//! (`pg_advisory_xact_lock`, auto-released at COMMIT/ROLLBACK) and
//! session-scoped (`pg_advisory_lock` / `pg_advisory_unlock`, held until
//! explicitly released or the connection closes). Redis locks used at the
//! 39 real call sites are held across logically-related-but-separate
//! operations (acquire, do work, release), not scoped to a single SQL
//! transaction, so this uses session-level locks to match that shape.
//!
//! **This means the same pooled connection must be held for the acquire →
//! release span** — advisory locks are connection-local, not
//! row/table-local, so returning the connection to the pool between
//! acquire and release (which a naive `get_conn(); lock(); return_conn();
//! ...; get_conn(); unlock();` pattern would do) releases nothing and lets
//! a different caller grab a different connection and believe it holds the
//! same lock. `PgLock` below holds the connection for its whole lifetime
//! for exactly this reason — do not "simplify" this by taking a fresh
//! connection per method call.
//!
//! Lock keys are hashed to Postgres's `bigint` advisory-lock key space with
//! the same `crc32fast` this codebase already uses for Redis shard-key
//! hashing (`KvStorePartition::partition_number` in `kv_store.rs`), not a
//! new hashing scheme — collisions are possible (32-bit hash space) and
//! unreviewed here; the real Redis locks use full string keys with no
//! collision risk, so this is a real, not cosmetic, semantic gap the
//! per-call-site audit needs to weigh for each of the 39 sites.
//!
//! Not done: lock-wait timeout / TTL-style auto-expiry (Redis locks here
//! are typically set with an expiry as a deadlock safety net; session
//! advisory locks have no built-in expiry — a held-and-abandoned lock
//! blocks forever unless the holding connection dies). A watchdog or
//! `statement_timeout`-based safety net is required before this is
//! deadlock-safe, and isn't built yet.
//!
//! Not compiled/tested against a real Postgres instance this session —
//! same caveat as `pg_kv_store.rs`.

use async_bb8_diesel::AsyncRunQueryDsl;
use bb8::PooledConnection;
use diesel::{sql_query, sql_types::BigInt, QueryableByName};
use error_stack::ResultExt;

use crate::errors::StorageError;
use crate::pg_kv_store::PgKvPool;

fn lock_key_to_bigint(key: &str) -> i64 {
    // crc32fast gives a u32; widen into i64's positive range so it round-trips
    // through Postgres's `bigint` bind type without sign trouble.
    i64::from(crc32fast::hash(key.as_bytes()))
}

#[derive(QueryableByName)]
struct AcquiredRow {
    #[diesel(sql_type = diesel::sql_types::Bool)]
    pg_try_advisory_lock: bool,
}

/// Holds a session-level Postgres advisory lock for its lifetime. Acquire
/// with `PgLock::try_acquire`, release with `.release().await`.
///
/// Borrows the pool for `'a` because the held connection must be the exact
/// one the lock was taken on (see module doc comment) — this is a real
/// lifetime, not decoration, and is why there is no owned/`'static` variant
/// of this type today.
pub struct PgLock<'a> {
    conn: PooledConnection<'a, async_bb8_diesel::ConnectionManager<diesel::PgConnection>>,
    key: i64,
}

impl<'a> PgLock<'a> {
    /// Analog of Redis's `SET key val NX EX ttl` used as a non-blocking
    /// try-lock. Returns `Ok(None)` (not an error) when another holder
    /// already has the lock — mirrors the existing call sites' pattern of
    /// treating "lock not acquired" as a normal branch, not a failure.
    pub async fn try_acquire(
        pool: &'a PgKvPool,
        key: &str,
    ) -> error_stack::Result<Option<Self>, StorageError> {
        let conn = pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;
        let numeric_key = lock_key_to_bigint(key);

        let rows: Vec<AcquiredRow> =
            sql_query("SELECT pg_try_advisory_lock($1) AS pg_try_advisory_lock")
                .bind::<BigInt, _>(numeric_key)
                .load_async(&conn)
                .await
                .map_err(StorageError::from)?;

        let acquired = rows.first().map(|r| r.pg_try_advisory_lock).unwrap_or(false);

        Ok(acquired.then_some(Self {
            conn,
            key: numeric_key,
        }))
    }

    pub async fn release(self) -> error_stack::Result<(), StorageError> {
        sql_query("SELECT pg_advisory_unlock($1)")
            .bind::<BigInt, _>(self.key)
            .execute_async(&self.conn)
            .await
            .map_err(StorageError::from)?;
        Ok(())
    }
}

// Deliberately no `impl Drop for PgLock` here: an async unlock can't run in
// a sync `drop()`, and returning the connection to the pool without calling
// `pg_advisory_unlock` first leaks the lock for the connection's remaining
// pooled lifetime (the next borrower inherits a lock it doesn't know it
// holds). Callers MUST call `.release()` explicitly, including on error
// paths — this is a real footgun carried over from the session-lock design,
// not fixed by this module, and needs a `PgLockGuard`-with-async-drop-shim
// (or a scoped-closure API) before this is safe to hand to call sites that
// don't already discipline themselves around explicit release.
