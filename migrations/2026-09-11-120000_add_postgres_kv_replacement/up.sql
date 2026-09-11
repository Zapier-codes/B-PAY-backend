-- Task 73/a — Postgres-backed replacement for the RedisStore choke-point
-- (crates/storage_impl/src/redis/{cache,kv_store,pub_sub}.rs).
--
-- Scope of THIS migration: the cache/kv-store half only (the "caching" and
-- "locking" call-site categories from Task 73/a's count: 29 + 39 of the 63
-- real Redis call sites). Pub/sub uses native LISTEN/NOTIFY and needs no
-- table for payloads under Postgres's 8000-byte NOTIFY limit; an overflow
-- table for larger payloads is added separately below so publish() has
-- somewhere to put a message id when a payload doesn't fit inline.
--
-- Design notes (why this shape, not a naive key/value table):
-- 1. Redis's per-key TTL + hash-field structure (HSET/HGET/HSCAN with a
--    shared per-key TTL applied via the KV wrapper's `ttl_for_kv`) is
--    modeled as one row per (key, field) so field-level HGET/HSCAN stay
--    single-row lookups instead of scanning and filtering a JSON blob.
-- 2. `SETNX`/`HSETNX` atomicity is preserved with Postgres's own atomicity
--    primitive for this: `INSERT ... ON CONFLICT DO NOTHING RETURNING`,
--    checked application-side for whether a row came back — no separate
--    read-then-write race window.
-- 3. Expired-but-not-yet-deleted rows are excluded via a WHERE clause at
--    read time (`expires_at IS NULL OR expires_at > now()`), the same
--    lazy-expiry semantics Redis itself uses for reads; a periodic sweep
--    (via the existing `scheduler` crate, not part of this migration) is
--    still needed to reclaim dead rows, since Postgres has no built-in TTL.

CREATE TABLE pg_kv_cache (
    id BIGSERIAL PRIMARY KEY,
    cache_key TEXT NOT NULL,
    field TEXT NOT NULL DEFAULT '',
    value BYTEA NOT NULL,
    expires_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT (now() AT TIME ZONE 'utc'),
    updated_at TIMESTAMP NOT NULL DEFAULT (now() AT TIME ZONE 'utc'),
    -- One row per (key, field); a plain (non-hash) key/value uses field = ''.
    CONSTRAINT pg_kv_cache_key_field_uniq UNIQUE (cache_key, field)
);

-- Every real lookup filters by cache_key (+ field for HGET, prefix scan for
-- HSCAN); expires_at is included so the not-expired filter can use the same
-- index instead of a second pass.
CREATE INDEX pg_kv_cache_key_idx ON pg_kv_cache (cache_key, expires_at);

-- Sweep target for the follow-up scheduler job (Task 73/a leaves the job
-- itself unwritten — see handover.md).
CREATE INDEX pg_kv_cache_expires_at_idx ON pg_kv_cache (expires_at)
    WHERE expires_at IS NOT NULL;

COMMENT ON TABLE pg_kv_cache IS
    'Postgres-backed replacement for RedisStore''s cache/kv_store.rs hash-field '
    'cache (Task 73/a). Row-per-field, ON CONFLICT DO NOTHING for SETNX/HSETNX '
    'atomicity, lazy expiry filtered at read time — no server-side TTL, needs '
    'a periodic sweep job.';

-- Overflow table for pub_sub.rs replacement: Postgres NOTIFY payloads are
-- capped at 8000 bytes, and several real pub/sub call sites (config
-- invalidation broadcasts) carry larger structured payloads than that.
-- publish() writes the payload here first and NOTIFYs with just the row id;
-- subscribers LISTEN, then SELECT the row by id on wake.
CREATE TABLE pg_pubsub_payload (
    id BIGSERIAL PRIMARY KEY,
    channel TEXT NOT NULL,
    payload BYTEA NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT (now() AT TIME ZONE 'utc')
);

CREATE INDEX pg_pubsub_payload_channel_idx ON pg_pubsub_payload (channel, created_at);

COMMENT ON TABLE pg_pubsub_payload IS
    'Overflow store for pub_sub.rs replacement (Task 73/a): holds payloads too '
    'large for a Postgres NOTIFY (8000-byte limit). NOTIFY carries only this '
    'table''s row id; subscribers fetch the real payload by id. Needs its own '
    'sweep job — rows are not deleted after delivery by this migration.';
