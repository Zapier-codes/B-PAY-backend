//! SQL layer of the Postgres backend.
//!
//! Everything here talks to the two tables created by
//! `migrations/2026-09-11-120000_add_postgres_kv_replacement`:
//!
//! * `pg_kv_cache (cache_key, field, value BYTEA, expires_at)` — one row per
//!   `(key, field)`. A plain Redis string uses `field = ''`; a Redis hash uses one
//!   row per field, all of them sharing the key's expiry.
//! * `pg_pubsub_payload (id, channel, payload BYTEA, created_at)` — the pub/sub log.
//!
//! Expiry is *lazy*, exactly like the SQL design note in that migration says: a row
//! whose `expires_at` has passed is invisible to every read here and is treated as
//! absent by the conditional writes (SETNX/HSETNX), while `sweep_pg_kv_cache()`
//! (pg_cron, migration `...130000...`) reclaims the space.
//!
//! All timestamps are computed by the database (`now() AT TIME ZONE 'utc'`, the
//! same convention as the table defaults) so instances with skewed clocks agree.

use std::{collections::HashMap, time::Duration};

use async_bb8_diesel::{AsyncRunQueryDsl, ConnectionManager};
use common_utils::errors::CustomResult;
use diesel::{
    sql_query,
    sql_types::{Array, BigInt, Binary, Bool, Nullable, Text},
    PgConnection, QueryableByName,
};
use error_stack::ResultExt;

/// Internal error; callers map it onto the `RedisError` variant of the command
/// they implement so the public error surface stays identical to the Redis backends.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreError {
    #[error("Failed to get a connection from the Postgres pool")]
    Pool,
    #[error("Postgres query failed")]
    Query,
}

type Pool = bb8::Pool<ConnectionManager<PgConnection>>;

/// `now()` as a UTC `timestamp` — the convention of the table's own defaults.
const NOW: &str = "(now() AT TIME ZONE 'utc')";
/// Row is visible (not yet expired).
const ALIVE: &str = "(expires_at IS NULL OR expires_at > (now() AT TIME ZONE 'utc'))";
/// Row is expired-but-not-yet-swept.
const DEAD: &str = "(expires_at IS NOT NULL AND expires_at <= (now() AT TIME ZONE 'utc'))";

/// `expires_at` expression for a nullable "seconds from now" bind parameter.
fn expiry(param: &str) -> String {
    format!(
        "CASE WHEN {param}::bigint IS NULL THEN NULL \
         ELSE {NOW} + make_interval(secs => {param}::double precision) END"
    )
}

#[derive(QueryableByName)]
struct ValueRow {
    #[diesel(sql_type = Binary)]
    value: Vec<u8>,
}

#[derive(QueryableByName)]
struct KeyValueRow {
    #[diesel(sql_type = Text)]
    cache_key: String,
    #[diesel(sql_type = Binary)]
    value: Vec<u8>,
}

#[derive(QueryableByName)]
struct FieldValueRow {
    #[diesel(sql_type = Text)]
    field: String,
    #[diesel(sql_type = Binary)]
    value: Vec<u8>,
}

#[derive(QueryableByName)]
struct IdRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

#[derive(QueryableByName)]
struct BoolRow {
    #[diesel(sql_type = Bool)]
    b: bool,
}

#[derive(QueryableByName)]
struct MessageRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
    #[diesel(sql_type = Text)]
    channel: String,
    #[diesel(sql_type = Binary)]
    payload: Vec<u8>,
}

pub(crate) struct PubSubRecord {
    pub id: i64,
    pub channel: String,
    pub payload: Vec<u8>,
}

pub(crate) struct PgStore {
    pool: Pool,
}

impl PgStore {
    /// Builds the pool and fails fast if the database is unreachable or the
    /// migrations have not been applied.
    pub(crate) async fn connect(
        url: &str,
        max_size: u32,
        connection_timeout: Duration,
    ) -> CustomResult<Self, StoreError> {
        let manager = ConnectionManager::<PgConnection>::new(url);
        let pool = bb8::Pool::builder()
            .max_size(max_size)
            .connection_timeout(connection_timeout)
            .build(manager)
            .await
            .change_context(StoreError::Pool)
            .attach_printable(
                "could not connect to the Postgres database in `redis.postgres_url`",
            )?;
        let store = Self { pool };
        store.ping().await?;
        Ok(store)
    }

    /// Cheap liveness + schema check.
    pub(crate) async fn ping(&self) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query("SELECT 1::bigint AS id FROM pg_kv_cache LIMIT 1")
            .load_async::<IdRow>(&*conn)
            .await
            .change_context(StoreError::Query)
            .attach_printable(
                "`pg_kv_cache` is not readable — apply migrations 2026-09-11-120000 and 2026-09-11-130000",
            )?;
        sql_query("SELECT 1::bigint AS id FROM pg_pubsub_payload LIMIT 1")
            .load_async::<IdRow>(&*conn)
            .await
            .change_context(StoreError::Query)
            .attach_printable(
                "`pg_pubsub_payload` is not readable — apply migration 2026-09-11-120000",
            )?;
        Ok(())
    }

    async fn conn(
        &self,
    ) -> CustomResult<bb8::PooledConnection<'_, ConnectionManager<PgConnection>>, StoreError> {
        self.pool.get().await.change_context(StoreError::Pool)
    }

    // ─── plain keys and single fields ────────────────────────────────────────

    /// `SET` / `SETEX` / `HSET` of one `(key, field)`. `ttl_secs = None` stores
    /// the value with no expiry.
    pub(crate) async fn put(
        &self,
        key: &str,
        field: &str,
        value: Vec<u8>,
        ttl_secs: Option<i64>,
    ) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query(format!(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, $3, {exp}) \
             ON CONFLICT (cache_key, field) DO UPDATE SET \
               value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, updated_at = {NOW}",
            exp = expiry("$4")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(field.to_owned())
        .bind::<Binary, _>(value)
        .bind::<Nullable<BigInt>, _>(ttl_secs)
        .execute_async(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(())
    }

    /// `SET ... KEEPTTL`: overwrite the value, keep the current expiry (a missing
    /// or already-expired row is created with no expiry, like Redis).
    pub(crate) async fn put_keep_ttl(
        &self,
        key: &str,
        field: &str,
        value: Vec<u8>,
    ) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query(format!(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, $3, NULL) \
             ON CONFLICT (cache_key, field) DO UPDATE SET \
               value = EXCLUDED.value, updated_at = {NOW}, \
               expires_at = CASE WHEN {dead} THEN NULL ELSE pg_kv_cache.expires_at END",
            dead = DEAD.replace("expires_at", "pg_kv_cache.expires_at")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(field.to_owned())
        .bind::<Binary, _>(value)
        .execute_async(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(())
    }

    /// `SET NX` / `HSETNX`: returns `true` iff this call stored the value. An
    /// expired-but-unswept row counts as absent and is overwritten.
    /// Atomic: a single `INSERT ... ON CONFLICT DO UPDATE ... WHERE expired RETURNING`.
    pub(crate) async fn put_nx(
        &self,
        key: &str,
        field: &str,
        value: Vec<u8>,
        ttl_secs: Option<i64>,
    ) -> CustomResult<bool, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, $3, {exp}) \
             ON CONFLICT (cache_key, field) DO UPDATE SET \
               value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, updated_at = {NOW} \
             WHERE {dead} \
             RETURNING id",
            exp = expiry("$4"),
            dead = DEAD.replace("expires_at", "pg_kv_cache.expires_at")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(field.to_owned())
        .bind::<Binary, _>(value)
        .bind::<Nullable<BigInt>, _>(ttl_secs)
        .load_async::<IdRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(!rows.is_empty())
    }

    pub(crate) async fn get(
        &self,
        key: &str,
        field: &str,
    ) -> CustomResult<Option<Vec<u8>>, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT value FROM pg_kv_cache WHERE cache_key = $1 AND field = $2 AND {ALIVE}"
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(field.to_owned())
        .load_async::<ValueRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.into_iter().next().map(|row| row.value))
    }

    /// `MGET` over plain keys. Missing/expired keys are absent from the map.
    pub(crate) async fn get_many(
        &self,
        keys: Vec<String>,
    ) -> CustomResult<HashMap<String, Vec<u8>>, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT cache_key, value FROM pg_kv_cache \
             WHERE cache_key = ANY($1) AND field = '' AND {ALIVE}"
        ))
        .bind::<Array<Text>, _>(keys)
        .load_async::<KeyValueRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows
            .into_iter()
            .map(|row| (row.cache_key, row.value))
            .collect())
    }

    /// `DEL`: removes the key with all of its fields. Returns how many live
    /// rows were removed (0 = the key did not exist).
    pub(crate) async fn delete_key(&self, key: &str) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "WITH d AS (DELETE FROM pg_kv_cache WHERE cache_key = $1 RETURNING expires_at) \
             SELECT count(*)::bigint AS n FROM d WHERE {ALIVE}"
        ))
        .bind::<Text, _>(key.to_owned())
        .load_async::<CountRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.n))
    }

    pub(crate) async fn exists(&self, key: &str) -> CustomResult<bool, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT EXISTS(SELECT 1 FROM pg_kv_cache WHERE cache_key = $1 AND {ALIVE}) AS b"
        ))
        .bind::<Text, _>(key.to_owned())
        .load_async::<BoolRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().is_some_and(|row| row.b))
    }

    /// `TTL`: -2 = no such key, -1 = no expiry, else remaining seconds.
    pub(crate) async fn ttl(&self, key: &str) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT CASE \
               WHEN count(*) = 0 THEN -2::bigint \
               WHEN bool_or(expires_at IS NULL) THEN -1::bigint \
               ELSE ceil(extract(epoch FROM (min(expires_at) - {NOW})))::bigint END AS n \
             FROM pg_kv_cache WHERE cache_key = $1 AND {ALIVE}"
        ))
        .bind::<Text, _>(key.to_owned())
        .load_async::<CountRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(-2, |row| row.n))
    }

    /// `EXPIRE`: sets the expiry of the whole key (every field).
    pub(crate) async fn expire_in(&self, key: &str, seconds: i64) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query(format!(
            "UPDATE pg_kv_cache SET expires_at = {exp} WHERE cache_key = $1 AND {ALIVE}",
            exp = expiry("$2")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Nullable<BigInt>, _>(Some(seconds))
        .execute_async(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(())
    }

    /// `EXPIREAT`: absolute unix timestamp (seconds).
    pub(crate) async fn expire_at(&self, key: &str, unix_ts: i64) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query(format!(
            "UPDATE pg_kv_cache SET expires_at = (to_timestamp($2::double precision) AT TIME ZONE 'utc') \
             WHERE cache_key = $1 AND {ALIVE}"
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<BigInt, _>(unix_ts)
        .execute_async(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(())
    }

    // ─── hashes ───────────────────────────────────────────────────────────────

    /// `HSET k f1 v1 f2 v2 ...` followed by `EXPIRE k ttl` — in one statement, so
    /// the whole key (including fields not in this batch) gets the new expiry
    /// atomically.
    pub(crate) async fn hset_many(
        &self,
        key: &str,
        fields: Vec<String>,
        values: Vec<Vec<u8>>,
        ttl_secs: i64,
    ) -> CustomResult<(), StoreError> {
        let conn = self.conn().await?;
        sql_query(format!(
            "WITH ins AS ( \
               INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
               SELECT $1::text, t.f, t.v, {exp} FROM unnest($2::text[], $3::bytea[]) AS t(f, v) \
               ON CONFLICT (cache_key, field) DO UPDATE SET \
                 value = EXCLUDED.value, expires_at = EXCLUDED.expires_at, updated_at = {NOW} \
               RETURNING 1) \
             UPDATE pg_kv_cache SET expires_at = {exp} \
             WHERE cache_key = $1 AND field <> ALL($2::text[])",
            exp = expiry("$4")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Array<Text>, _>(fields)
        .bind::<Array<Binary>, _>(values)
        .bind::<Nullable<BigInt>, _>(Some(ttl_secs))
        .execute_async(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(())
    }

    /// `HGETALL` (plain `field = ''` rows are not hash fields and are excluded).
    pub(crate) async fn hget_all(
        &self,
        key: &str,
    ) -> CustomResult<Vec<(String, Vec<u8>)>, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT field, value FROM pg_kv_cache \
             WHERE cache_key = $1 AND field <> '' AND {ALIVE} ORDER BY field"
        ))
        .bind::<Text, _>(key.to_owned())
        .load_async::<FieldValueRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.into_iter().map(|row| (row.field, row.value)).collect())
    }

    /// `HDEL`: number of live fields removed.
    pub(crate) async fn hdel(
        &self,
        key: &str,
        fields: Vec<String>,
    ) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "WITH d AS (DELETE FROM pg_kv_cache \
                        WHERE cache_key = $1 AND field = ANY($2) AND field <> '' \
                        RETURNING expires_at) \
             SELECT count(*)::bigint AS n FROM d WHERE {ALIVE}"
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Array<Text>, _>(fields)
        .load_async::<CountRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.n))
    }

    /// `HINCRBY` — a single atomic upsert; returns the value after the increment.
    /// A missing or expired field starts from 0 (and, like Redis, no TTL is set).
    /// Fails (Query error) if the existing value is not an integer.
    pub(crate) async fn hincr(
        &self,
        key: &str,
        field: &str,
        by: i64,
    ) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
             VALUES ($1, $2, convert_to($3::bigint::text, 'UTF8'), NULL) \
             ON CONFLICT (cache_key, field) DO UPDATE SET \
               value = convert_to( \
                 (CASE WHEN {dead} THEN 0 ELSE convert_from(pg_kv_cache.value, 'UTF8')::bigint END \
                  + $3::bigint)::text, 'UTF8'), \
               expires_at = CASE WHEN {dead} THEN NULL ELSE pg_kv_cache.expires_at END, \
               updated_at = {NOW} \
             RETURNING convert_from(value, 'UTF8')::bigint AS n",
            dead = DEAD.replace("expires_at", "pg_kv_cache.expires_at")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(field.to_owned())
        .bind::<BigInt, _>(by)
        .load_async::<CountRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.n))
    }

    /// `HSCAN key MATCH <glob>`: values of the live fields whose name matches
    /// the already-converted LIKE pattern.
    pub(crate) async fn hscan_values(
        &self,
        key: &str,
        like_pattern: &str,
    ) -> CustomResult<Vec<Vec<u8>>, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "SELECT value FROM pg_kv_cache \
             WHERE cache_key = $1 AND field <> '' AND field LIKE $2 ESCAPE '\\' AND {ALIVE} \
             ORDER BY field"
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Text, _>(like_pattern.to_owned())
        .load_async::<ValueRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.into_iter().map(|row| row.value).collect())
    }

    // ─── sets ─────────────────────────────────────────────────────────────────

    /// `SADD`: members are stored as fields with an empty value. Returns the number
    /// of members that were newly added.
    pub(crate) async fn sadd(
        &self,
        key: &str,
        members: Vec<String>,
    ) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(format!(
            "WITH ins AS ( \
               INSERT INTO pg_kv_cache (cache_key, field, value, expires_at) \
               SELECT $1::text, m, ''::bytea, NULL FROM unnest($2::text[]) AS m \
               ON CONFLICT (cache_key, field) DO UPDATE SET \
                 value = EXCLUDED.value, expires_at = NULL, updated_at = {NOW} \
                 WHERE {dead} \
               RETURNING 1) \
             SELECT count(*)::bigint AS n FROM ins",
            dead = DEAD.replace("expires_at", "pg_kv_cache.expires_at")
        ))
        .bind::<Text, _>(key.to_owned())
        .bind::<Array<Text>, _>(members)
        .load_async::<CountRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.n))
    }

    // ─── pub/sub ──────────────────────────────────────────────────────────────

    /// Appends to the pub/sub log; returns the message id.
    pub(crate) async fn publish(
        &self,
        channel: &str,
        payload: Vec<u8>,
    ) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(
            "INSERT INTO pg_pubsub_payload (channel, payload) VALUES ($1, $2) RETURNING id",
        )
        .bind::<Text, _>(channel.to_owned())
        .bind::<Binary, _>(payload)
        .load_async::<IdRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.id))
    }

    /// Highest message id currently in the log (0 when empty).
    pub(crate) async fn max_message_id(&self) -> CustomResult<i64, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query("SELECT COALESCE(max(id), 0)::bigint AS id FROM pg_pubsub_payload")
            .load_async::<IdRow>(&*conn)
            .await
            .change_context(StoreError::Query)?;
        Ok(rows.first().map_or(0, |row| row.id))
    }

    /// Messages published after `after_id`, oldest first.
    pub(crate) async fn poll_messages(
        &self,
        after_id: i64,
    ) -> CustomResult<Vec<PubSubRecord>, StoreError> {
        let conn = self.conn().await?;
        let rows = sql_query(
            "SELECT id, channel, payload FROM pg_pubsub_payload \
             WHERE id > $1 ORDER BY id LIMIT 500",
        )
        .bind::<BigInt, _>(after_id)
        .load_async::<MessageRow>(&*conn)
        .await
        .change_context(StoreError::Query)?;
        Ok(rows
            .into_iter()
            .map(|row| PubSubRecord {
                id: row.id,
                channel: row.channel,
                payload: row.payload,
            })
            .collect())
    }
}

/// Converts a Redis glob (`*`, `?`) to a SQL `LIKE` pattern (escape char `\`).
pub(crate) fn glob_to_like(glob: &str) -> String {
    let mut like = String::with_capacity(glob.len());
    for character in glob.chars() {
        match character {
            '*' => like.push('%'),
            '?' => like.push('_'),
            '%' | '_' | '\\' => {
                like.push('\\');
                like.push(character);
            }
            other => like.push(other),
        }
    }
    like
}

#[cfg(test)]
mod tests {
    use super::glob_to_like;

    #[test]
    fn glob_conversion() {
        assert_eq!(glob_to_like("*"), "%");
        assert_eq!(glob_to_like("pa_*?"), "pa\\_%_");
        assert_eq!(glob_to_like("100%"), "100\\%");
        assert_eq!(glob_to_like("a\\b"), "a\\\\b");
    }
}
