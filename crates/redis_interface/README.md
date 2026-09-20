# Redis Interface

A user-friendly interface to Redis.

## Postgres (Supabase) backend — no Redis server

Besides `redis-rs` (default) and `fred`, the crate has a `postgres` backend
(`--no-default-features --features postgres`). It keeps the public API identical, so
nothing that depends on `redis_interface` changes, but stores everything in the
`pg_kv_cache` and `pg_pubsub_payload` tables and never opens a Redis connection.

* **Setup**: apply `migrations/2026-09-11-120000_add_postgres_kv_replacement` (tables)
  and, optionally, `...130000_add_pg_kv_cache_pubsub_sweep_jobs` (pg_cron sweeps), then
  set `redis.postgres_url` (env `ROUTER__REDIS__POSTGRES_URL`) to a Supabase
  *session-mode* / direct connection string (`...:5432/postgres?sslmode=require`;
  the transaction pooler on 6543 has no prepared-statement support).
* **Semantics**: TTLs are lazy (expired rows are invisible and treated as absent by
  `SETNX`); `SETNX`/`HSETNX`/`HINCRBY` are single atomic statements; pub/sub is a
  polled log (`~250 ms` delivery) so it works through connection poolers.
* **Not supported**: Redis streams and consumer groups (KV drainer, scheduler queue,
  the `RedisKv` merchant storage scheme). Those calls return a `RedisError` that says so.
* **Tests**: `REDIS_INTERFACE_PG_TEST_URL=postgres://... cargo test -p redis_interface
  --no-default-features --features postgres` (tests are skipped without the variable).
