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
//! **FNV-1a, 64-bit** (see `lock_key_to_bigint` below) — **fixed this
//! session (audit correction, see `handover.md`'s "Task 73/a — audit
//! correction" entry); the original draft used `crc32fast` widened from a
//! 32-bit hash into `i64`, an unreviewed collision risk flagged in that
//! same audit.** FNV-1a's 64-bit output uses the *full* `bigint` key space
//! Postgres advisory locks are keyed on, matching the collision profile the
//! real Redis locks have (full string keys, no collision risk) far more
//! closely than a 32-bit hash did. Deliberately not `std`'s
//! `DefaultHasher`/`SipHash` either — its exact output is only promised
//! stable within a single build, which is fine for ephemeral advisory-lock
//! keys computed and consumed within one running binary, but FNV-1a is a
//! fully specified, dependency-free algorithm with no such caveat, so
//! there's no reason to take the weaker guarantee. Still probabilistic
//! (any hash into a fixed space is), so still worth naming in the
//! per-call-site audit — just a 2^32-times-smaller risk than the draft this
//! replaces.
//!
//! **Lock-wait timeout / auto-expiry — also fixed this session, partially.**
//! Redis locks here are typically set with an expiry as a deadlock safety
//! net; session-level Postgres advisory locks have no built-in expiry of
//! their own. `try_acquire` now additionally runs `SET idle_session_timeout`
//! (Postgres 14+/Supabase-supported GUC) on the held connection once the
//! lock is actually acquired: session advisory locks are released
//! automatically when their owning backend terminates (documented Postgres
//! behaviour), so if a caller forgets `.release()` (or panics before
//! reaching it) and the connection goes back to the pool unreleased, once
//! that connection then sits genuinely idle for
//! [`LOCK_IDLE_SESSION_TIMEOUT_SECS`] Postgres kills the backend itself and
//! the lock frees — turning the previously-unbounded leak into a bounded
//! one. **Caveat, stated plainly, not hidden:** this does not bound a lock
//! held while the pool keeps actively reusing that same connection for
//! other queries (each query resets the idle timer), only the
//! genuinely-idle-and-forgotten case the module doc comment originally
//! flagged. A full fix — e.g. a `PgLockGuard` with an async-drop shim that
//! force-releases on drop regardless of connection reuse — needs an owned,
//! non-lifetime-borrowed connection type bb8 0.8 doesn't hand out (`get()`
//! only returns a `PooledConnection<'_, M>` tied to the pool's borrow), and
//! this workspace forbids `unsafe_code` at the lint level, so no
//! self-referential-struct workaround either; that redesign is real,
//! separate follow-up work, not done here.
//!
//! **Multi-key locking — fixed this session (Finding #1, `handover.md`'s
//! Task 73/a audit).** `LockAction::HoldMultiple` (`core/api_locking.rs`)
//! has no single-key equivalent to fall back on; this used to have no
//! entry point at all. `try_acquire_multiple` below acquires every key in
//! a batch on one held connection via non-blocking `pg_try_advisory_lock`
//! per key, rolling back anything already acquired the instant one key in
//! the batch is unavailable — see that method's own doc comment for why
//! this reproduces Redis's all-or-nothing batch semantics without risking
//! the sequential-blocking-lock deadlock shape a naive port would have.
//! `try_acquire` (single key) is now just `try_acquire_multiple` with a
//! one-element slice — no behaviour change for existing single-key
//! callers.
//!
//! Not compiled/tested against a real Postgres instance — checked this
//! session (network access to `crates.io` is available in-sandbox, so this
//! was actually attempted, not just assumed away): the workspace pins
//! `rust-version = "1.85.0"` and the only toolchain installable from this
//! sandbox's allowed package sources is Ubuntu's apt `rustc` 1.75.0 — too
//! old to build this workspace, and there's no network path from here to
//! install a newer one. Still a real gap, now a *diagnosed* one instead of
//! an assumed one; whoever next has a 1.85+ toolchain available should
//! `cargo check -p storage_impl` before this is trusted further — the new
//! `try_acquire_multiple` method is reviewed by reading only, same
//! standing caveat as every other change in this file.

use async_bb8_diesel::AsyncRunQueryDsl;
use bb8::PooledConnection;
use diesel::{sql_query, sql_types::BigInt, QueryableByName};
use error_stack::ResultExt;

use crate::{errors::StorageError, pg_kv_store::PgKvPool};

/// How long a held lock's connection may sit genuinely idle in the pool
/// before Postgres kills that backend and frees any advisory lock it still
/// holds — the safety net for a caller that never called `.release()`. See
/// the module doc comment's caveats before relying on this as a full fix.
const LOCK_IDLE_SESSION_TIMEOUT_SECS: u32 = 30;

fn lock_key_to_bigint(key: &str) -> i64 {
    // FNV-1a, 64-bit. Fully specified, dependency-free, deterministic
    // across processes/builds — see module doc comment for why this
    // replaced the earlier crc32fast-widened-to-i64 scheme.
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // Advisory-lock keys are an opaque 64-bit space; sign doesn't matter,
    // this is a lossless bit-reinterpretation, not a truncation.
    #[allow(clippy::as_conversions)]
    {
        hash as i64
    }
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
    // Sorted, deduplicated set of every advisory-lock key this instance
    // currently holds. A single-key `try_acquire` is just the len-1 case of
    // `try_acquire_multiple` (see below) — no behaviour change for existing
    // single-key callers, just a shared representation.
    keys: Vec<i64>,
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
        Self::try_acquire_multiple(pool, &[key]).await
    }

    /// Multi-key analog of `try_acquire`, backing `LockAction::HoldMultiple`
    /// (see `core/api_locking.rs::perform_locking_action` — Finding #1 in
    /// the per-call-site audit, `handover.md`).
    ///
    /// Redis's `set_multiple_keys_if_not_exists_and_get_values` acquires
    /// every key in **one** atomic server-side round trip: if any key is
    /// already held, none of them count as this caller's lock and nothing
    /// is left held. Postgres session advisory locks have no "acquire N or
    /// none" primitive, so this reproduces the same all-or-nothing result
    /// with `pg_try_advisory_lock` called once per key **on a single held
    /// connection** (a session can hold any number of advisory locks
    /// simultaneously — no need for one connection per key), rolling back
    /// every key already acquired *this call* the instant one key fails,
    /// instead of leaving a partial lock set held.
    ///
    /// **Why this can't deadlock, unlike acquiring keys one at a time with
    /// a blocking lock:** `pg_try_advisory_lock` never blocks — a caller
    /// either gets a key immediately or moves on. Two callers racing over
    /// the same two keys in opposite order can therefore never each end up
    /// holding one key while waiting on the other (the textbook multi-lock
    /// deadlock shape Finding #1 named); the loser of any single key in the
    /// batch fails fast, releases what it already grabbed here, and the
    /// existing retry-with-delay loop at the `HoldMultiple` call site tries
    /// the whole batch again later — the same backoff-and-retry shape
    /// Redis's version already uses, not new behaviour introduced here.
    ///
    /// Keys are also sorted (independent of the caller's own order) as
    /// defence in depth: it costs nothing, and it means a consistent lock
    /// order is applied for free even if the underlying acquisition
    /// strategy ever changes later — it isn't load-bearing for correctness
    /// today, since the non-blocking property above already rules out the
    /// cyclic-wait deadlock condition on its own.
    pub async fn try_acquire_multiple(
        pool: &'a PgKvPool,
        keys: &[&str],
    ) -> error_stack::Result<Option<Self>, StorageError> {
        let mut numeric_keys: Vec<i64> = keys.iter().map(|key| lock_key_to_bigint(key)).collect();
        numeric_keys.sort_unstable();
        numeric_keys.dedup();

        let conn = pool
            .get()
            .await
            .change_context(StorageError::DatabaseConnectionError)?;

        let mut acquired_keys: Vec<i64> = Vec::with_capacity(numeric_keys.len());
        for numeric_key in numeric_keys {
            let rows: Vec<AcquiredRow> =
                sql_query("SELECT pg_try_advisory_lock($1) AS pg_try_advisory_lock")
                    .bind::<BigInt, _>(numeric_key)
                    .load_async(&conn)
                    .await
                    .map_err(StorageError::from)?;

            let acquired = rows
                .first()
                .map(|r| r.pg_try_advisory_lock)
                .unwrap_or(false);

            if acquired {
                acquired_keys.push(numeric_key);
                continue;
            }

            // One key in the batch is already held elsewhere: this call
            // does not get "its" lock on any of them, matching the Redis
            // fencing-token check's all-or-nothing semantics. Roll back
            // every key acquired earlier in this same loop before
            // returning — otherwise a partial lock set would sit held
            // until this connection's idle-session-timeout safety net (or
            // its own lifetime) eventually frees it, unnecessarily
            // blocking unrelated callers in the meantime.
            for key_to_release in acquired_keys.iter().rev() {
                if let Err(error) = sql_query("SELECT pg_advisory_unlock($1)")
                    .bind::<BigInt, _>(*key_to_release)
                    .execute_async(&conn)
                    .await
                {
                    router_env::logger::warn!(
                        ?error,
                        key = *key_to_release,
                        "pg_lock: rollback pg_advisory_unlock failed while backing out of a \
                         partially-acquired multi-key lock; this key stays held on this \
                         connection until its idle-session-timeout safety net frees it"
                    );
                }
            }

            return Ok(None);
        }

        // Every key in the batch acquired cleanly. Safety net for a caller
        // that never reaches `.release()` — see module doc comment. Value
        // is a compile-time constant, not caller input, so it's inlined
        // directly rather than bound: Postgres's `SET` grammar doesn't
        // accept a query parameter in the value position (`SET x = $1` is a
        // syntax error over the extended protocol), only a literal.
        let idle_timeout_sql =
            format!("SET idle_session_timeout = '{LOCK_IDLE_SESSION_TIMEOUT_SECS}s'");
        sql_query(idle_timeout_sql)
            .execute_async(&conn)
            .await
            .map_err(StorageError::from)?;

        Ok(Some(Self {
            conn,
            keys: acquired_keys,
        }))
    }

    pub async fn release(self) -> error_stack::Result<(), StorageError> {
        for key in &self.keys {
            sql_query("SELECT pg_advisory_unlock($1)")
                .bind::<BigInt, _>(*key)
                .execute_async(&self.conn)
                .await
                .map_err(StorageError::from)?;
        }

        // Clear the safety-net timeout set in `try_acquire`/
        // `try_acquire_multiple` before this connection goes back to the
        // pool — otherwise it leaks onto whichever unrelated caller
        // borrows this same pooled connection next. Best-effort: if this
        // fails, the connection just keeps a (more conservative, not less
        // safe) idle timeout it shouldn't — not worth failing an otherwise-
        // successful release over.
        if let Err(error) = sql_query("RESET idle_session_timeout")
            .execute_async(&self.conn)
            .await
        {
            // Matches this crate's existing structured-logging convention
            // (see e.g. `merchant_connector_account.rs`), not an ad hoc
            // format string.
            router_env::logger::warn!(
                ?error,
                "pg_lock: RESET idle_session_timeout failed after release, connection keeps a stale timeout until next reuse resets it"
            );
        }

        Ok(())
    }
}

// Deliberately still no `impl Drop for PgLock` here: an async unlock can't
// run in a sync `drop()`, and bb8 0.8's `PooledConnection<'a, M>` borrows
// the pool for `'a` rather than handing out an owned/'static connection, so
// there's no sound way to spawn a background task that finishes the async
// unlock on drop without either an owned connection type this pool doesn't
// provide or `unsafe_code` (forbidden workspace-wide by this crate's own
// lints). Returning the connection to the pool without calling
// `pg_advisory_unlock` first now costs at most
// `LOCK_IDLE_SESSION_TIMEOUT_SECS` of leaked-lock time instead of the
// connection's entire remaining pooled lifetime (see `try_acquire`'s
// `SET idle_session_timeout` and the module doc comment's caveats on what
// that safety net does and doesn't cover) — bounded now, not eliminated.
// Callers MUST still call `.release()` explicitly, including on error
// paths; a `PgLockGuard`-with-async-drop-shim (or a scoped-closure API)
// backed by an owned connection is the real fix and still isn't built.
