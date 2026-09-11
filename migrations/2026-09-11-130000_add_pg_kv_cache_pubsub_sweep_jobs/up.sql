-- Task 73/a -- expiry-sweep jobs for pg_kv_cache and pg_pubsub_payload.
--
-- Both tables were flagged at creation time
-- (2026-09-11-120000_add_postgres_kv_replacement/up.sql) as needing a
-- periodic reclaim job: Postgres has no server-side TTL, so rows past
-- their lazy-expiry cutoff (pg_kv_cache.expires_at) or past their
-- delivery window (pg_pubsub_payload, which has no expires_at column
-- at all) accumulate forever otherwise. handover.md calls this out
-- for both tables as an unbounded-growth gap, most recently for
-- pg_pubsub_payload specifically in the pg_kv_store.rs LIKE-escape
-- session.
--
-- Mechanism: pg_cron, already the mechanism Task 73's own scope
-- decision names for the Kafka/ClickHouse replacement, and a
-- Supabase-supported managed extension (no separate worker process to
-- deploy/monitor). Each sweep runs on its own schedule via a plpgsql
-- function rather than a bare DELETE scheduled directly, for one
-- reason worth stating plainly: this is the FIRST time either table
-- gets swept, ever -- an unknown, possibly large backlog exists right
-- now, and cron.schedule has no built-in per-call row cap. A single
-- unbounded `DELETE ... WHERE expires_at < now()` against however many
-- rows have accumulated since Task 73/a shipped would hold its row
-- locks and generate WAL for one unpredictably long transaction on its
-- very first tick. Batching bounds that: each function call deletes at
-- most `batch_size` rows per statement and loops up to `max_batches`
-- times, leaving any remainder for the next scheduled tick instead of
-- trying to clear an unbounded backlog in one go.

CREATE EXTENSION IF NOT EXISTS pg_cron;

-- ---------------------------------------------------------------------
-- pg_kv_cache: delete rows past their own expires_at. NULL expires_at
-- rows (if any caller ever writes one) are never touched -- absence of
-- an expiry is a deliberate "does not expire" signal, not an oversight
-- to sweep away.
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION sweep_pg_kv_cache(
    batch_size INT DEFAULT 5000,
    max_batches INT DEFAULT 20
) RETURNS BIGINT AS $$
DECLARE
    deleted_this_batch INT;
    total_deleted BIGINT := 0;
    batches_run INT := 0;
BEGIN
    LOOP
        DELETE FROM pg_kv_cache
        WHERE id IN (
            SELECT id FROM pg_kv_cache
            WHERE expires_at IS NOT NULL
              AND expires_at < (now() AT TIME ZONE 'utc')
            LIMIT batch_size
        );
        GET DIAGNOSTICS deleted_this_batch = ROW_COUNT;
        total_deleted := total_deleted + deleted_this_batch;
        batches_run := batches_run + 1;
        EXIT WHEN deleted_this_batch < batch_size OR batches_run >= max_batches;
    END LOOP;
    RETURN total_deleted;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION sweep_pg_kv_cache(INT, INT) IS
    'Deletes expired pg_kv_cache rows in bounded batches (default '
    '5000/batch x 20 batches = 100k-row ceiling per call). Scheduled via '
    'pg_cron below (every 5 min); safe to call manually with a larger '
    'max_batches for a one-off backlog drain the first time this runs '
    'against a project that already has a real backlog.';

-- ---------------------------------------------------------------------
-- pg_pubsub_payload: no expires_at column -- these rows exist only to
-- carry a payload too large for Postgres's 8000-byte NOTIFY limit from
-- publish() to whichever LISTEN-ing subscribers wake on that NOTIFY.
-- Every real subscriber reads its row within the same request cycle
-- that raised the NOTIFY (typically sub-second), not on some later
-- schedule -- so age since created_at, not a per-row expiry, is the
-- right signal. Retention is deliberately generous (10 minutes)
-- relative to that real read pattern, to leave room for a slow or
-- momentarily-disconnected subscriber to still catch up before its row
-- disappears out from under it.
-- ---------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS pg_pubsub_payload_created_at_idx
    ON pg_pubsub_payload (created_at);

CREATE OR REPLACE FUNCTION sweep_pg_pubsub_payload(
    retention_interval INTERVAL DEFAULT '10 minutes',
    batch_size INT DEFAULT 5000,
    max_batches INT DEFAULT 20
) RETURNS BIGINT AS $$
DECLARE
    deleted_this_batch INT;
    total_deleted BIGINT := 0;
    batches_run INT := 0;
BEGIN
    LOOP
        DELETE FROM pg_pubsub_payload
        WHERE id IN (
            SELECT id FROM pg_pubsub_payload
            WHERE created_at < (now() AT TIME ZONE 'utc') - retention_interval
            LIMIT batch_size
        );
        GET DIAGNOSTICS deleted_this_batch = ROW_COUNT;
        total_deleted := total_deleted + deleted_this_batch;
        batches_run := batches_run + 1;
        EXIT WHEN deleted_this_batch < batch_size OR batches_run >= max_batches;
    END LOOP;
    RETURN total_deleted;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION sweep_pg_pubsub_payload(INTERVAL, INT, INT) IS
    'Deletes pg_pubsub_payload rows older than retention_interval '
    '(default 10 minutes -- generous vs. the sub-second real read '
    'pattern), in bounded batches. Scheduled via pg_cron below (every '
    '2 min -- tighter than the kv sweep since this table''s natural '
    'write rate is one row per publish() call, not one per cache write).';

-- ---------------------------------------------------------------------
-- Schedule both sweeps. Unschedule-by-name first: cron.schedule() is
-- not idempotent on its own -- re-running this migration without the
-- unschedule step would register a second, duplicate job under a new
-- jobid every time, rather than replacing the existing one.
-- ---------------------------------------------------------------------
SELECT cron.unschedule(jobid) FROM cron.job WHERE jobname = 'sweep_pg_kv_cache';
SELECT cron.unschedule(jobid) FROM cron.job WHERE jobname = 'sweep_pg_pubsub_payload';

SELECT cron.schedule('sweep_pg_kv_cache', '*/5 * * * *', 'SELECT sweep_pg_kv_cache()');
SELECT cron.schedule('sweep_pg_pubsub_payload', '*/2 * * * *', 'SELECT sweep_pg_pubsub_payload()');
