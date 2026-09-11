SELECT cron.unschedule(jobid) FROM cron.job WHERE jobname = 'sweep_pg_kv_cache';
SELECT cron.unschedule(jobid) FROM cron.job WHERE jobname = 'sweep_pg_pubsub_payload';

DROP FUNCTION IF EXISTS sweep_pg_kv_cache(INT, INT);
DROP FUNCTION IF EXISTS sweep_pg_pubsub_payload(INTERVAL, INT, INT);
DROP INDEX IF EXISTS pg_pubsub_payload_created_at_idx;

-- pg_cron itself is deliberately not dropped here -- it's a shared,
-- cluster-level extension; if another job depends on it by the time
-- this migration is ever rolled back, dropping it here would take
-- that job down too. Only what this migration created is undone.
