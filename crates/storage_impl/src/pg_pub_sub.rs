//! Postgres LISTEN/NOTIFY replacement for `redis/pub_sub.rs` — the 18-file
//! pub/sub category of Task 73/a's 63 real Redis call sites.
//!
//! ## Why an overflow table instead of raw NOTIFY payloads
//! Postgres caps a `NOTIFY` payload at 8000 bytes (server-enforced, not
//! configurable). Several real pub/sub call sites broadcast structured
//! config-invalidation messages that this session did not size-audit
//! against that limit, so this always writes the payload to
//! `pg_pubsub_payload` first and NOTIFYs with only the row id — same
//! store-then-notify pattern used for outbox-style delivery elsewhere, and
//! it sidesteps the size question entirely rather than needing every
//! caller to prove their payload fits.
//!
//! ## What this does not replicate
//! Redis pub/sub is fire-and-forget with no delivery guarantee to
//! subscribers who weren't listening at publish time; so is Postgres
//! NOTIFY — that part matches. What does NOT match: `bb8`/diesel connection
//! pooling is designed around short-lived borrowed connections, while
//! `LISTEN` needs one connection held open for the listener's entire
//! lifetime, same "don't return this connection to the pool" constraint as
//! `pg_lock.rs`. A real subscriber needs a dedicated, non-pooled connection
//! (or a pool carved out and reserved for listeners) — not built here; the
//! function below sketches the query shape (`LISTEN`, then poll
//! `pg_pubsub_payload` for new rows since a last-seen id) without solving
//! connection lifecycle, and should not be treated as ready for a real
//! subscriber loop.
//!
//! ## Cleanup
//! `publish()` inserts a row on every call and nothing in this file ever
//! deletes it — flagged in an earlier session as the same unbounded-growth
//! shape `pg_kv_cache` had before its own sweep job. **Fixed this session**,
//! DB-side rather than in this module: a scheduled `sweep_pg_pubsub_payload()`
//! function (bounded-batch delete by `created_at` age, every 2 minutes) — see
//! `migrations/2026-09-11-130000_add_pg_kv_cache_pubsub_sweep_jobs/up.sql`.
//! Nothing in this file changed; the sweep is pure SQL/pg_cron and has no
//! Rust-side dependency.

use async_bb8_diesel::AsyncRunQueryDsl;
use diesel::{sql_query, sql_types::Text, QueryableByName};
use error_stack::ResultExt;

use crate::{errors::StorageError, pg_kv_store::PgKvPool};

// diesel's QueryableByName derive fully-qualifies its generated sql_type
// path (e.g. diesel::sql_types::Binary) and, for a single-field struct,
// that generated qualification gets misattributed to this field's own
// span under -D warnings -- not a real redundant-path issue in code we
// wrote. Silenced narrowly rather than at the crate/lint-group level.
// Confirmed against a real failed CI run (2026-09-12, cargo check -p
// storage_impl pinned 1.85.0 job); NOT recompiled locally -- no working
// rustc >=1.85 in this sandbox (see legacy-node/handover.md New-Clone
// Checklist). Flag as reviewed-by-reading only until a session with a
// real toolchain confirms.
#[allow(unused_qualifications)]
#[derive(QueryableByName)]
struct InsertedPayloadRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    id: i64,
}

/// Analog of `pub_sub.rs`'s publish. Stores the payload, then NOTIFYs with
/// just the row id — see the module doc comment for why.
pub async fn publish(
    pool: &PgKvPool,
    channel: &str,
    payload: &[u8],
) -> error_stack::Result<(), StorageError> {
    let conn = pool
        .get()
        .await
        .change_context(StorageError::DatabaseConnectionError)?;

    let rows: Vec<InsertedPayloadRow> =
        sql_query("INSERT INTO pg_pubsub_payload (channel, payload) VALUES ($1, $2) RETURNING id")
            .bind::<Text, _>(channel.to_owned())
            .bind::<diesel::sql_types::Binary, _>(payload.to_vec())
            .load_async(&*conn)
            .await
            .map_err(StorageError::from)?;

    let id = rows.first().map(|r| r.id).ok_or_else(|| {
        error_stack::report!(StorageError::ValueNotFound(
            "pg_pubsub_payload insert returned no row".to_string()
        ))
    })?;

    // `pg_notify` (function form) is used instead of literal `NOTIFY
    // channel, 'payload'` so the channel name can be a bound parameter —
    // NOTIFY's own SQL syntax doesn't allow that.
    sql_query("SELECT pg_notify($1, $2)")
        .bind::<Text, _>(channel.to_owned())
        .bind::<Text, _>(id.to_string())
        .execute_async(&*conn)
        .await
        .map_err(StorageError::from)?;

    Ok(())
}

// Subscriber-side (LISTEN + payload fetch-by-id loop) intentionally left
// unimplemented — see the module doc comment's connection-lifecycle note.
// The 18 real call sites' subscriber shapes haven't been audited yet
// either, so writing a generic loop here risks guessing an API shape that
// doesn't fit what they actually need.
