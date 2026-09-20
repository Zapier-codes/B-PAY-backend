//! The Redis command surface, implemented on Postgres.
//!
//! Signatures deliberately match `module/redis_rs/commands.rs` one-to-one (same
//! generic bounds, same return types, same `RedisError` variants) so that no call
//! site changes. The `redis` crate is used purely as a *codec*: values reach us as
//! `redis::ToRedisArgs` and are turned into bytes for the `BYTEA` column, and
//! bytes read back are decoded through `redis::FromRedisValue` — the same
//! conversions a real Redis round-trip performs, including `Nil` for a missing key
//! (so `get_key::<Vec<u8>>` on a missing key is empty and `get_key::<String>`
//! fails, exactly as before).

use std::fmt::Debug;

use common_utils::{
    errors::CustomResult,
    ext_traits::{ByteSliceExt, Encode, StringExt},
    fp_utils,
};
use error_stack::{report, ResultExt};
use redis::{FromRedisValue, ToSingleRedisArg};

use super::store::{glob_to_like, PgStore};
use crate::{
    errors,
    types::{
        DelReply, HsetnxReply, RedisEntryId, RedisKey, SaddReply, SetGetReply, SetnxReply,
        StreamReadResult, StreamTrimConfig,
    },
};

/// First (only) argument a single-value `ToRedisArgs` type produces.
fn encode_arg<V: redis::ToRedisArgs>(value: &V) -> Vec<u8> {
    value.to_redis_args().into_iter().next().unwrap_or_default()
}

fn to_text(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Decodes stored bytes the way a Redis reply would be: `None` is `Nil`.
fn decode<V: FromRedisValue>(bytes: Option<Vec<u8>>) -> Result<V, redis::ParsingError> {
    V::from_redis_value(match bytes {
        Some(bytes) => redis::Value::BulkString(bytes),
        None => redis::Value::Nil,
    })
}

fn unsupported<T>(
    error: errors::RedisError,
    operation: &str,
) -> CustomResult<T, errors::RedisError> {
    Err(report!(error).attach_printable(format!(
        "`{operation}` is not supported by the Postgres backend (Redis streams / consumer groups \
         back the KV drainer and the scheduler queue and have no Postgres implementation yet)"
    )))
}

impl super::RedisConnectionWithContext {
    /// Prefix `key` with the tenant key prefix of the underlying pool.
    pub fn add_prefix(&self, key: &str) -> String {
        self.redis_conn.add_prefix(key)
    }

    fn store(&self) -> &PgStore {
        &self.redis_conn.store
    }

    fn physical_key(&self, key: &RedisKey) -> String {
        key.tenant_aware_key(&self.redis_conn)
    }

    // ─── Key Commands ────────────────────────────────────────────────────────

    pub async fn set_key<V>(&self, key: &RedisKey, value: V) -> CustomResult<(), errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + Send + Sync + ToSingleRedisArg,
    {
        self.store()
            .put(
                &self.physical_key(key),
                "",
                encode_arg(&value),
                Some(i64::from(self.redis_conn.config.default_ttl)),
            )
            .await
            .change_context(errors::RedisError::SetFailed)
    }

    pub async fn set_key_without_modifying_ttl<V>(
        &self,
        key: &RedisKey,
        value: V,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + Send + Sync + ToSingleRedisArg,
    {
        self.store()
            .put_keep_ttl(&self.physical_key(key), "", encode_arg(&value))
            .await
            .change_context(errors::RedisError::SetFailed)
    }

    pub async fn serialize_and_set_key_if_not_exist<V>(
        &self,
        key: &RedisKey,
        value: V,
        ttl: Option<i64>,
    ) -> CustomResult<SetnxReply, errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let serialized = value
            .encode_to_vec()
            .change_context(errors::RedisError::JsonSerializationFailed)?;
        self.set_key_if_not_exists_with_expiry(key, serialized.as_slice(), ttl)
            .await
    }

    pub async fn serialize_and_set_key<V>(
        &self,
        key: &RedisKey,
        value: V,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let serialized = value
            .encode_to_vec()
            .change_context(errors::RedisError::JsonSerializationFailed)?;
        self.set_key(key, serialized.as_slice()).await
    }

    pub async fn serialize_and_set_key_without_modifying_ttl<V>(
        &self,
        key: &RedisKey,
        value: V,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let serialized = value
            .encode_to_vec()
            .change_context(errors::RedisError::JsonSerializationFailed)?;
        self.set_key_without_modifying_ttl(key, serialized.as_slice())
            .await
    }

    pub async fn serialize_and_set_key_with_expiry<V>(
        &self,
        key: &RedisKey,
        value: V,
        seconds: i64,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let serialized = value
            .encode_to_vec()
            .change_context(errors::RedisError::JsonSerializationFailed)?;
        self.set_key_with_expiry(key, serialized.as_slice(), seconds)
            .await
    }

    pub async fn get_key<V>(&self, key: &RedisKey) -> CustomResult<V, errors::RedisError>
    where
        V: FromRedisValue + Send + 'static,
    {
        let stored = self
            .store()
            .get(&self.physical_key(key), "")
            .await
            .change_context(errors::RedisError::GetFailed)?;
        decode::<V>(stored).change_context(errors::RedisError::GetFailed)
    }

    pub async fn get_multiple_keys<V>(
        &self,
        keys: &[RedisKey],
    ) -> CustomResult<Vec<Option<V>>, errors::RedisError>
    where
        V: FromRedisValue + Send + 'static,
    {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let physical_keys: Vec<String> = keys.iter().map(|key| self.physical_key(key)).collect();
        let mut stored = self
            .store()
            .get_many(physical_keys.clone())
            .await
            .change_context(errors::RedisError::GetFailed)?;

        physical_keys
            .iter()
            .map(|physical_key| match stored.remove(physical_key) {
                Some(bytes) => decode::<V>(Some(bytes))
                    .map(Some)
                    .change_context(errors::RedisError::GetFailed),
                None => Ok(None),
            })
            .collect()
    }

    pub async fn exists<V>(&self, key: &RedisKey) -> CustomResult<bool, errors::RedisError>
    where
        V: Send + 'static,
    {
        self.store()
            .exists(&self.physical_key(key))
            .await
            .change_context(errors::RedisError::GetFailed)
    }

    pub async fn get_and_deserialize_key<T>(
        &self,
        key: &RedisKey,
        type_name: &'static str,
    ) -> CustomResult<T, errors::RedisError>
    where
        T: serde::de::DeserializeOwned,
    {
        let value_bytes = self.get_key::<Vec<u8>>(key).await?;
        fp_utils::when(value_bytes.is_empty(), || Err(errors::RedisError::NotFound))?;
        value_bytes
            .parse_struct(type_name)
            .change_context(errors::RedisError::JsonDeserializationFailed)
    }

    pub async fn get_and_deserialize_multiple_keys<T>(
        &self,
        keys: &[RedisKey],
        type_name: &'static str,
    ) -> CustomResult<Vec<Option<T>>, errors::RedisError>
    where
        T: serde::de::DeserializeOwned,
    {
        let value_bytes_vec = self.get_multiple_keys::<Vec<u8>>(keys).await?;

        let mut results = Vec::with_capacity(value_bytes_vec.len());
        for value_bytes_opt in value_bytes_vec {
            match value_bytes_opt {
                Some(value_bytes) if !value_bytes.is_empty() => {
                    let parsed = value_bytes
                        .parse_struct(type_name)
                        .change_context(errors::RedisError::JsonDeserializationFailed)?;
                    results.push(Some(parsed));
                }
                _ => results.push(None),
            }
        }
        Ok(results)
    }

    pub async fn delete_key(&self, key: &RedisKey) -> CustomResult<DelReply, errors::RedisError> {
        let deleted = self
            .store()
            .delete_key(&self.physical_key(key))
            .await
            .change_context(errors::RedisError::DeleteFailed)?;
        Ok(if deleted > 0 {
            DelReply::KeyDeleted
        } else {
            DelReply::KeyNotDeleted
        })
    }

    pub async fn delete_multiple_keys(
        &self,
        keys: &[RedisKey],
    ) -> CustomResult<Vec<DelReply>, errors::RedisError> {
        let futures = keys.iter().map(|key| self.delete_key(key));
        futures::future::try_join_all(futures)
            .await
            .change_context(errors::RedisError::DeleteFailed)
    }

    pub async fn set_key_with_expiry<V>(
        &self,
        key: &RedisKey,
        value: V,
        seconds: i64,
    ) -> CustomResult<(), errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + Send + Sync + ToSingleRedisArg,
    {
        u64::try_from(seconds).change_context(errors::RedisError::SetExFailed)?;
        self.store()
            .put(
                &self.physical_key(key),
                "",
                encode_arg(&value),
                Some(seconds),
            )
            .await
            .change_context(errors::RedisError::SetExFailed)
    }

    pub async fn set_key_if_not_exists_with_expiry<V>(
        &self,
        key: &RedisKey,
        value: V,
        seconds: Option<i64>,
    ) -> CustomResult<SetnxReply, errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + Send + Sync + ToSingleRedisArg,
    {
        let ttl = seconds.unwrap_or(self.redis_conn.config.default_ttl.into());
        u64::try_from(ttl).change_context(errors::RedisError::SetFailed)?;
        let inserted = self
            .store()
            .put_nx(&self.physical_key(key), "", encode_arg(&value), Some(ttl))
            .await
            .change_context(errors::RedisError::SetFailed)?;
        Ok(if inserted {
            SetnxReply::KeySet
        } else {
            SetnxReply::KeyNotSet
        })
    }

    pub async fn set_expiry(
        &self,
        key: &RedisKey,
        seconds: i64,
    ) -> CustomResult<(), errors::RedisError> {
        self.store()
            .expire_in(&self.physical_key(key), seconds)
            .await
            .change_context(errors::RedisError::SetExpiryFailed)
    }

    pub async fn set_expire_at(
        &self,
        key: &RedisKey,
        timestamp: i64,
    ) -> CustomResult<(), errors::RedisError> {
        self.store()
            .expire_at(&self.physical_key(key), timestamp)
            .await
            .change_context(errors::RedisError::SetExpiryFailed)
    }

    pub async fn get_ttl(&self, key: &RedisKey) -> CustomResult<i64, errors::RedisError> {
        self.store()
            .ttl(&self.physical_key(key))
            .await
            .change_context(errors::RedisError::GetFailed)
    }

    // ─── Hash Commands ───────────────────────────────────────────────────────

    pub async fn set_hash_fields<F, V>(
        &self,
        key: &RedisKey,
        field_value_pairs: Vec<(F, V)>,
        ttl: Option<i64>,
    ) -> CustomResult<(), errors::RedisError>
    where
        F: redis::ToRedisArgs + Debug + Send + Sync,
        V: redis::ToRedisArgs + Debug + Send + Sync,
    {
        if field_value_pairs.is_empty() {
            return Ok(());
        }
        let (fields, values): (Vec<String>, Vec<Vec<u8>>) = field_value_pairs
            .iter()
            .map(|(field, value)| (to_text(encode_arg(field)), encode_arg(value)))
            .unzip();
        self.store()
            .hset_many(
                &self.physical_key(key),
                fields,
                values,
                ttl.unwrap_or(self.redis_conn.config.default_hash_ttl.into()),
            )
            .await
            .change_context(errors::RedisError::SetHashFailed)
    }

    pub async fn set_hash_field_if_not_exist<V>(
        &self,
        key: &RedisKey,
        field: &str,
        value: V,
        ttl: Option<u32>,
    ) -> CustomResult<HsetnxReply, errors::RedisError>
    where
        V: redis::ToRedisArgs + ToSingleRedisArg + Debug + Send + Sync,
    {
        let inserted = self
            .store()
            .put_nx(&self.physical_key(key), field, encode_arg(&value), None)
            .await
            .change_context(errors::RedisError::SetHashFieldFailed)?;

        if !inserted {
            return Ok(HsetnxReply::KeyNotSet);
        }
        // Only set expiry if the field was actually set
        self.set_expiry(
            key,
            ttl.unwrap_or(self.redis_conn.config.default_hash_ttl)
                .into(),
        )
        .await?;
        Ok(HsetnxReply::KeySet)
    }

    pub async fn serialize_and_set_hash_field_if_not_exist<V>(
        &self,
        key: &RedisKey,
        field: &str,
        value: V,
        ttl: Option<u32>,
    ) -> CustomResult<HsetnxReply, errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let serialized = value
            .encode_to_vec()
            .change_context(errors::RedisError::JsonSerializationFailed)?;
        self.set_hash_field_if_not_exist(key, field, serialized.as_slice(), ttl)
            .await
    }

    pub async fn serialize_and_set_multiple_hash_field_if_not_exist<V>(
        &self,
        kv: &[(&RedisKey, V)],
        field: &str,
        ttl: Option<u32>,
    ) -> CustomResult<Vec<HsetnxReply>, errors::RedisError>
    where
        V: serde::Serialize + Debug,
    {
        let mut hsetnx: Vec<HsetnxReply> = Vec::with_capacity(kv.len());
        for (key, val) in kv {
            hsetnx.push(
                self.serialize_and_set_hash_field_if_not_exist(key, field, val, ttl)
                    .await?,
            );
        }
        Ok(hsetnx)
    }

    pub async fn increment_fields_in_hash<T>(
        &self,
        key: &RedisKey,
        fields_to_increment: &[(T, i64)],
    ) -> CustomResult<Vec<usize>, errors::RedisError>
    where
        T: Debug + ToString,
    {
        let physical_key = self.physical_key(key);
        let mut values_after_increment = Vec::with_capacity(fields_to_increment.len());
        for (field, increment) in fields_to_increment {
            let value = self
                .store()
                .hincr(&physical_key, &field.to_string(), *increment)
                .await
                .change_context(errors::RedisError::IncrementHashFieldFailed)?;
            values_after_increment.push(
                usize::try_from(value)
                    .change_context(errors::RedisError::IncrementHashFieldFailed)?,
            );
        }
        Ok(values_after_increment)
    }

    /// Values of the hash fields whose name matches the glob `pattern`
    /// (`*` and `?` are supported). `count` is a Redis iteration hint and is ignored.
    pub async fn hscan(
        &self,
        key: &RedisKey,
        pattern: &str,
        _count: Option<u32>,
    ) -> CustomResult<Vec<String>, errors::RedisError> {
        let values = self
            .store()
            .hscan_values(&self.physical_key(key), &glob_to_like(pattern))
            .await
            .change_context(errors::RedisError::GetHashFieldFailed)?;
        Ok(values
            .into_iter()
            .filter_map(|bytes| String::from_utf8(bytes).ok())
            .collect())
    }

    pub async fn hscan_and_deserialize<T>(
        &self,
        key: &RedisKey,
        pattern: &str,
        count: Option<u32>,
    ) -> CustomResult<Vec<T>, errors::RedisError>
    where
        T: serde::de::DeserializeOwned,
    {
        let redis_results = self.hscan(key, pattern, count).await?;
        Ok(redis_results
            .iter()
            .filter_map(|redis_value| {
                let r: T = redis_value.parse_struct(std::any::type_name::<T>()).ok()?;
                Some(r)
            })
            .collect())
    }

    pub async fn get_hash_field<V>(
        &self,
        key: &RedisKey,
        field: &str,
    ) -> CustomResult<V, errors::RedisError>
    where
        V: FromRedisValue + Send + 'static,
    {
        let stored = self
            .store()
            .get(&self.physical_key(key), field)
            .await
            .change_context(errors::RedisError::GetHashFieldFailed)?;
        decode::<V>(stored).change_context(errors::RedisError::GetHashFieldFailed)
    }

    pub async fn get_hash_fields<V>(&self, key: &RedisKey) -> CustomResult<V, errors::RedisError>
    where
        V: FromRedisValue + Send + 'static,
    {
        let fields = self
            .store()
            .hget_all(&self.physical_key(key))
            .await
            .change_context(errors::RedisError::GetHashFieldFailed)?;
        let map = redis::Value::Map(
            fields
                .into_iter()
                .map(|(field, value)| {
                    (
                        redis::Value::BulkString(field.into_bytes()),
                        redis::Value::BulkString(value),
                    )
                })
                .collect(),
        );
        V::from_redis_value(map).change_context(errors::RedisError::GetHashFieldFailed)
    }

    pub async fn get_hash_field_and_deserialize<V>(
        &self,
        key: &RedisKey,
        field: &str,
        type_name: &'static str,
    ) -> CustomResult<V, errors::RedisError>
    where
        V: serde::de::DeserializeOwned,
    {
        let value_bytes = self.get_hash_field::<Vec<u8>>(key, field).await?;

        if value_bytes.is_empty() {
            return Err(errors::RedisError::NotFound.into());
        }

        value_bytes
            .parse_struct(type_name)
            .change_context(errors::RedisError::JsonDeserializationFailed)
    }

    pub async fn delete_hash_fields<F>(
        &self,
        key: &RedisKey,
        fields: F,
    ) -> CustomResult<usize, errors::RedisError>
    where
        F: redis::ToRedisArgs + Debug + Send + Sync,
    {
        let fields: Vec<String> = fields.to_redis_args().into_iter().map(to_text).collect();
        let removed = self
            .store()
            .hdel(&self.physical_key(key), fields)
            .await
            .change_context(errors::RedisError::DeleteHashFieldFailed)?;
        usize::try_from(removed).change_context(errors::RedisError::DeleteHashFieldFailed)
    }

    // ─── Set Commands ────────────────────────────────────────────────────────

    pub async fn sadd<V>(
        &self,
        key: &RedisKey,
        members: V,
    ) -> CustomResult<SaddReply, errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + Send + Sync,
    {
        let members: Vec<String> = members.to_redis_args().into_iter().map(to_text).collect();
        let added = self
            .store()
            .sadd(&self.physical_key(key), members)
            .await
            .change_context(errors::RedisError::SetAddMembersFailed)?;
        Ok(if added > 0 {
            SaddReply::KeySet(added)
        } else {
            SaddReply::KeyNotSet
        })
    }

    // ─── Conditional set + read ──────────────────────────────────────────────

    pub async fn set_multiple_keys_if_not_exists_and_get_values<V>(
        &self,
        keys: &[(RedisKey, V)],
        ttl: Option<i64>,
    ) -> CustomResult<Vec<SetGetReply<V>>, errors::RedisError>
    where
        V: redis::ToRedisArgs
            + Debug
            + FromRedisValue
            + ToOwned<Owned = V>
            + Send
            + Sync
            + serde::de::DeserializeOwned,
    {
        let futures = keys.iter().map(|(key, value)| {
            self.set_key_if_not_exists_and_get_value(key, (*value).to_owned(), ttl)
        });

        futures::future::try_join_all(futures)
            .await
            .change_context(errors::RedisError::SetFailed)
    }

    /// Sets the value if the key is absent and returns the value that is stored
    /// afterwards (the new one, or the pre-existing one).
    ///
    /// The conditional write is atomic; the read that follows is a second statement,
    /// so a concurrent delete/expiry between the two surfaces as `SetFailed`.
    pub async fn set_key_if_not_exists_and_get_value<V>(
        &self,
        key: &RedisKey,
        value: V,
        ttl: Option<i64>,
    ) -> CustomResult<SetGetReply<V>, errors::RedisError>
    where
        V: redis::ToRedisArgs + Debug + FromRedisValue + Send + Sync + serde::de::DeserializeOwned,
    {
        let physical_key = self.physical_key(key);
        let ttl_seconds = ttl.unwrap_or(self.redis_conn.config.default_ttl.into());

        let inserted = self
            .store()
            .put_nx(&physical_key, "", encode_arg(&value), Some(ttl_seconds))
            .await
            .change_context(errors::RedisError::SetFailed)?;
        let stored = self
            .store()
            .get(&physical_key, "")
            .await
            .change_context(errors::RedisError::SetFailed)?;
        let actual_value: V = decode(stored)
            .change_context(errors::RedisError::SetFailed)
            .attach_printable("Failed to convert from stored value")?;

        Ok(if inserted {
            SetGetReply::ValueSet(actual_value)
        } else {
            SetGetReply::ValueExists(actual_value)
        })
    }

    // ─── Streams & consumer groups: not available on Postgres ────────────────

    #[allow(clippy::unused_async)]
    pub async fn stream_append_entry<F, V>(
        &self,
        _stream: &RedisKey,
        _entry_id: &RedisEntryId,
        _fields: Vec<(F, V)>,
    ) -> CustomResult<(), errors::RedisError>
    where
        F: Into<String> + Debug + Send + Sync,
        V: Into<String> + Debug + Send + Sync,
    {
        unsupported(
            errors::RedisError::StreamAppendFailed,
            "stream_append_entry",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_delete_entries(
        &self,
        _stream: &RedisKey,
        _ids: Vec<String>,
    ) -> CustomResult<usize, errors::RedisError> {
        unsupported(
            errors::RedisError::StreamDeleteFailed,
            "stream_delete_entries",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_trim_entries(
        &self,
        _stream: &RedisKey,
        _config: StreamTrimConfig,
    ) -> CustomResult<usize, errors::RedisError> {
        unsupported(errors::RedisError::StreamTrimFailed, "stream_trim_entries")
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_acknowledge_entries(
        &self,
        _stream: &RedisKey,
        _group: &str,
        _ids: Vec<String>,
    ) -> CustomResult<usize, errors::RedisError> {
        unsupported(
            errors::RedisError::StreamAcknowledgeFailed,
            "stream_acknowledge_entries",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_get_length(
        &self,
        _stream: &RedisKey,
    ) -> CustomResult<usize, errors::RedisError> {
        unsupported(errors::RedisError::GetLengthFailed, "stream_get_length")
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_read_entries(
        &self,
        _streams: &[RedisKey],
        _ids: Vec<String>,
        _read_count: Option<u64>,
    ) -> CustomResult<StreamReadResult, errors::RedisError> {
        unsupported(errors::RedisError::StreamReadFailed, "stream_read_entries")
    }

    #[allow(clippy::unused_async)]
    pub async fn stream_read_with_options(
        &self,
        _streams: &[RedisKey],
        _ids: Vec<String>,
        _count: Option<u64>,
        _block: Option<u64>,
        _group: Option<(&str, &str)>,
    ) -> CustomResult<StreamReadResult, errors::RedisError> {
        unsupported(
            errors::RedisError::StreamReadFailed,
            "stream_read_with_options",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn consumer_group_create(
        &self,
        _stream: &RedisKey,
        _group: &str,
        _id: &RedisEntryId,
    ) -> CustomResult<(), errors::RedisError> {
        unsupported(
            errors::RedisError::ConsumerGroupCreateFailed,
            "consumer_group_create",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn consumer_group_destroy(
        &self,
        _stream: &RedisKey,
        _group: &str,
    ) -> CustomResult<crate::types::ConsumerGroupDestroyReply, errors::RedisError> {
        unsupported(
            errors::RedisError::ConsumerGroupDestroyFailed,
            "consumer_group_destroy",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn consumer_group_delete_consumer(
        &self,
        _stream: &RedisKey,
        _group: &str,
        _consumer: &str,
    ) -> CustomResult<usize, errors::RedisError> {
        unsupported(
            errors::RedisError::ConsumerGroupRemoveConsumerFailed,
            "consumer_group_delete_consumer",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn consumer_group_set_last_id(
        &self,
        _stream: &RedisKey,
        _group: &str,
        _id: &RedisEntryId,
    ) -> CustomResult<String, errors::RedisError> {
        unsupported(
            errors::RedisError::ConsumerGroupSetIdFailed,
            "consumer_group_set_last_id",
        )
    }

    #[allow(clippy::unused_async)]
    pub async fn consumer_group_set_message_owner<R>(
        &self,
        _stream: &RedisKey,
        _group: &str,
        _consumer: &str,
        _min_idle_time: u64,
        _ids: Vec<String>,
    ) -> CustomResult<R, errors::RedisError>
    where
        R: FromRedisValue + Send + 'static,
    {
        unsupported(
            errors::RedisError::ConsumerGroupClaimFailed,
            "consumer_group_set_message_owner",
        )
    }
}
