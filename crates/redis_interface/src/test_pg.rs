//! Tests for the Postgres backend.
//!
//! They need a database that has the `pg_kv_cache` / `pg_pubsub_payload` tables
//! (`migrations/2026-09-11-120000_add_postgres_kv_replacement`). Point
//! `REDIS_INTERFACE_PG_TEST_URL` at one, e.g.
//!
//! ```text
//! REDIS_INTERFACE_PG_TEST_URL=postgres://user:pass@localhost:5432/db \
//!     cargo test -p redis_interface --no-default-features --features postgres
//! ```
//!
//! Without the variable every test returns early (so `cargo test` stays green on
//! machines with no database). Each test namespaces its keys with a unique tenant
//! prefix, so the tests can share one database and run in parallel.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_qualifications
)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::{
    errors::RedisError,
    types::{
        DelReply, HsetnxReply, PostgresUrl, RedisEntryId, RedisKey, RedisSettings, SaddReply,
        SetGetReply, SetnxReply,
    },
    RedisConnectionPool, RedisConnectionWithContext, RedisValue,
};

fn test_url() -> Option<String> {
    std::env::var("REDIS_INTERFACE_PG_TEST_URL").ok()
}

fn settings(url: &str) -> RedisSettings {
    RedisSettings {
        postgres_url: PostgresUrl::new(url),
        pool_size: 4,
        ..RedisSettings::default()
    }
}

/// A connection whose keys are isolated from every other test.
async fn connect(name: &str) -> Option<RedisConnectionWithContext> {
    let url = test_url()?;
    let pool = RedisConnectionPool::new_without_event_emitter(&settings(&url))
        .await
        .expect("connect to the test database");
    let prefix = format!(
        "t{}_{}_{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    Some(RedisConnectionWithContext::new_without_context(Arc::new(
        pool.clone(&prefix),
    )))
}

fn key(name: &str) -> RedisKey {
    name.into()
}

#[tokio::test]
async fn set_get_delete_roundtrip() {
    let Some(conn) = connect("roundtrip").await else {
        return;
    };
    let k = key("k");
    conn.set_key(&k, "hello").await.unwrap();
    assert_eq!(conn.get_key::<String>(&k).await.unwrap(), "hello");
    assert_eq!(
        conn.get_key::<Vec<u8>>(&k).await.unwrap(),
        b"hello".to_vec()
    );
    assert!(conn.exists::<()>(&k).await.unwrap());

    // overwrite
    conn.set_key(&k, "world").await.unwrap();
    assert_eq!(conn.get_key::<String>(&k).await.unwrap(), "world");

    assert_eq!(conn.delete_key(&k).await.unwrap(), DelReply::KeyDeleted);
    assert_eq!(conn.delete_key(&k).await.unwrap(), DelReply::KeyNotDeleted);
    assert!(!conn.exists::<()>(&k).await.unwrap());
}

#[tokio::test]
async fn missing_key_decodes_like_redis_nil() {
    let Some(conn) = connect("missing").await else {
        return;
    };
    let k = key("nope");
    // A `String` cannot be built from Nil -> error, exactly as with Redis.
    assert!(conn.get_key::<String>(&k).await.is_err());
    // `Vec<u8>` / `Option<_>` accept Nil.
    assert!(conn.get_key::<Vec<u8>>(&k).await.unwrap().is_empty());
    assert_eq!(conn.get_key::<Option<String>>(&k).await.unwrap(), None);
    // and the JSON helper turns "empty" into NotFound.
    let err = conn
        .get_and_deserialize_key::<Vec<i32>>(&k, "Vec<i32>")
        .await
        .unwrap_err();
    assert_eq!(*err.current_context(), RedisError::NotFound);
}

#[tokio::test]
async fn expiry_is_lazy_and_observed_by_every_read() {
    let Some(conn) = connect("expiry").await else {
        return;
    };
    let k = key("short");
    conn.set_key_with_expiry(&k, "v", 1).await.unwrap();
    assert!(conn.exists::<()>(&k).await.unwrap());
    let ttl = conn.get_ttl(&k).await.unwrap();
    assert!((0..=1).contains(&ttl), "ttl was {ttl}");

    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert!(!conn.exists::<()>(&k).await.unwrap());
    assert!(conn.get_key::<Option<String>>(&k).await.unwrap().is_none());
    assert_eq!(conn.get_ttl(&k).await.unwrap(), -2);
    // an expired row does not count as a deletion
    assert_eq!(conn.delete_key(&k).await.unwrap(), DelReply::KeyNotDeleted);
}

#[tokio::test]
async fn ttl_codes_match_redis() {
    let Some(conn) = connect("ttl").await else {
        return;
    };
    let k = key("k");
    assert_eq!(conn.get_ttl(&k).await.unwrap(), -2, "missing key");

    conn.set_key_with_expiry(&k, "v", 100).await.unwrap();
    let ttl = conn.get_ttl(&k).await.unwrap();
    assert!((99..=100).contains(&ttl), "ttl was {ttl}");

    // KEEPTTL keeps it
    conn.set_key_without_modifying_ttl(&k, "v2").await.unwrap();
    let ttl = conn.get_ttl(&k).await.unwrap();
    assert!((98..=100).contains(&ttl), "ttl was {ttl}");
    assert_eq!(conn.get_key::<String>(&k).await.unwrap(), "v2");

    conn.set_expiry(&k, 500).await.unwrap();
    let ttl = conn.get_ttl(&k).await.unwrap();
    assert!((499..=500).contains(&ttl), "ttl was {ttl}");

    // EXPIREAT with an absolute timestamp ~ 300s from now
    let at = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
        + 300;
    conn.set_expire_at(&k, at).await.unwrap();
    let ttl = conn.get_ttl(&k).await.unwrap();
    assert!((298..=300).contains(&ttl), "ttl was {ttl}");

    // KEEPTTL on a brand-new key creates it without expiry
    let fresh = key("fresh");
    conn.set_key_without_modifying_ttl(&fresh, "x")
        .await
        .unwrap();
    assert_eq!(conn.get_ttl(&fresh).await.unwrap(), -1);
}

#[tokio::test]
async fn setnx_semantics_including_expired_rows() {
    let Some(conn) = connect("setnx").await else {
        return;
    };
    let k = key("lock");
    assert_eq!(
        conn.set_key_if_not_exists_with_expiry(&k, "a", Some(1))
            .await
            .unwrap(),
        SetnxReply::KeySet
    );
    assert_eq!(
        conn.set_key_if_not_exists_with_expiry(&k, "b", Some(1))
            .await
            .unwrap(),
        SetnxReply::KeyNotSet
    );
    assert_eq!(conn.get_key::<String>(&k).await.unwrap(), "a");

    // once the first holder has expired, the next caller wins (row is replaced)
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert_eq!(
        conn.set_key_if_not_exists_with_expiry(&k, "c", Some(30))
            .await
            .unwrap(),
        SetnxReply::KeySet
    );
    assert_eq!(conn.get_key::<String>(&k).await.unwrap(), "c");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setnx_has_exactly_one_winner_under_contention() {
    let Some(conn) = connect("contention").await else {
        return;
    };
    let mut handles = Vec::new();
    for i in 0..16 {
        let conn = conn.clone();
        handles.push(tokio::spawn(async move {
            conn.set_key_if_not_exists_with_expiry(&key("race"), format!("w{i}"), Some(30))
                .await
                .unwrap()
        }));
    }
    let mut winners = 0;
    for handle in handles {
        if handle.await.unwrap() == SetnxReply::KeySet {
            winners += 1;
        }
    }
    assert_eq!(winners, 1);
}

#[tokio::test]
async fn json_helpers() {
    let Some(conn) = connect("json").await else {
        return;
    };
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Doc {
        id: u32,
        name: String,
    }
    let doc = Doc {
        id: 7,
        name: "seven".into(),
    };
    let k = key("doc");
    conn.serialize_and_set_key(&k, &doc).await.unwrap();
    assert_eq!(
        conn.get_and_deserialize_key::<Doc>(&k, "Doc")
            .await
            .unwrap(),
        doc
    );

    conn.serialize_and_set_key_with_expiry(&k, &doc, 60)
        .await
        .unwrap();
    conn.serialize_and_set_key_without_modifying_ttl(
        &k,
        &Doc {
            id: 8,
            name: "eight".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        conn.get_and_deserialize_key::<Doc>(&k, "Doc")
            .await
            .unwrap()
            .id,
        8
    );
    assert!(conn.get_ttl(&k).await.unwrap() > 0, "ttl kept");

    let k2 = key("doc2");
    assert_eq!(
        conn.serialize_and_set_key_if_not_exist(&k2, &doc, Some(30))
            .await
            .unwrap(),
        SetnxReply::KeySet
    );
    assert_eq!(
        conn.serialize_and_set_key_if_not_exist(&k2, &doc, Some(30))
            .await
            .unwrap(),
        SetnxReply::KeyNotSet
    );

    let many = conn
        .get_and_deserialize_multiple_keys::<Doc>(&[k.clone(), key("absent"), k2.clone()], "Doc")
        .await
        .unwrap();
    assert_eq!(many.len(), 3);
    assert!(many[0].is_some() && many[1].is_none() && many[2].is_some());
}

#[tokio::test]
async fn router_health_check_sequence() {
    // The exact call sequence of `health_check_redis` in the router, including
    // `get_key::<()>` (a `()` accepts any reply).
    let Some(conn) = connect("health").await else {
        return;
    };
    let k = key("test_key");
    conn.serialize_and_set_key_with_expiry(&k, "test_value", 30)
        .await
        .unwrap();
    conn.get_key::<()>(&k).await.unwrap();
    assert_eq!(conn.delete_key(&k).await.unwrap(), DelReply::KeyDeleted);
}

#[tokio::test]
async fn multiple_key_helpers() {
    let Some(conn) = connect("multi").await else {
        return;
    };
    conn.set_key(&key("a"), "1").await.unwrap();
    conn.set_key(&key("c"), "3").await.unwrap();
    let got = conn
        .get_multiple_keys::<String>(&[key("a"), key("b"), key("c")])
        .await
        .unwrap();
    assert_eq!(
        got,
        vec![Some("1".to_string()), None, Some("3".to_string())]
    );
    assert!(conn
        .get_multiple_keys::<String>(&[])
        .await
        .unwrap()
        .is_empty());

    let del = conn
        .delete_multiple_keys(&[key("a"), key("b"), key("c")])
        .await
        .unwrap();
    assert_eq!(
        del,
        vec![
            DelReply::KeyDeleted,
            DelReply::KeyNotDeleted,
            DelReply::KeyDeleted
        ]
    );
}

#[tokio::test]
async fn hash_commands() {
    let Some(conn) = connect("hash").await else {
        return;
    };
    let h = key("h");
    conn.set_hash_fields(
        &h,
        vec![("f1", "v1"), ("f2", "v2"), ("g1", "w1")],
        Some(120),
    )
    .await
    .unwrap();

    assert_eq!(conn.get_hash_field::<String>(&h, "f1").await.unwrap(), "v1");
    assert!(conn.get_hash_field::<String>(&h, "zzz").await.is_err());
    assert_eq!(
        conn.get_hash_field::<Option<String>>(&h, "zzz")
            .await
            .unwrap(),
        None
    );

    let all: HashMap<String, String> = conn.get_hash_fields(&h).await.unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(all["g1"], "w1");

    // glob match on field names
    let mut matched = conn.hscan(&h, "f*", Some(100)).await.unwrap();
    matched.sort();
    assert_eq!(matched, vec!["v1".to_string(), "v2".to_string()]);
    assert_eq!(conn.hscan(&h, "f?", None).await.unwrap().len(), 2);
    assert_eq!(
        conn.hscan(&h, "g1", None).await.unwrap(),
        vec!["w1".to_string()]
    );

    // whole key shares the expiry
    let ttl = conn.get_ttl(&h).await.unwrap();
    assert!((118..=120).contains(&ttl), "ttl was {ttl}");

    // a second HSET re-expires *every* field, including the ones not in the batch
    conn.set_hash_fields(&h, vec![("f1", "v1b")], Some(400))
        .await
        .unwrap();
    assert_eq!(
        conn.get_hash_field::<String>(&h, "f1").await.unwrap(),
        "v1b"
    );
    let ttl = conn.get_ttl(&h).await.unwrap();
    assert!((398..=400).contains(&ttl), "ttl was {ttl}");

    assert_eq!(conn.delete_hash_fields(&h, "f1").await.unwrap(), 1);
    assert_eq!(
        conn.delete_hash_fields(&h, vec!["f2", "nope"])
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        conn.get_hash_fields::<HashMap<String, String>>(&h)
            .await
            .unwrap()
            .len(),
        1
    );

    // DEL removes the whole hash
    assert_eq!(conn.delete_key(&h).await.unwrap(), DelReply::KeyDeleted);
    assert!(conn
        .get_hash_fields::<HashMap<String, String>>(&h)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn hash_setnx_and_json() {
    let Some(conn) = connect("hsetnx").await else {
        return;
    };
    let h = key("h");
    assert_eq!(
        conn.serialize_and_set_hash_field_if_not_exist(&h, "pi", &vec![1, 2, 3], Some(90))
            .await
            .unwrap(),
        HsetnxReply::KeySet
    );
    assert_eq!(
        conn.serialize_and_set_hash_field_if_not_exist(&h, "pi", &vec![9], Some(90))
            .await
            .unwrap(),
        HsetnxReply::KeyNotSet
    );
    let ttl = conn.get_ttl(&h).await.unwrap();
    assert!((88..=90).contains(&ttl), "ttl was {ttl}");
    assert_eq!(
        conn.get_hash_field_and_deserialize::<Vec<i32>>(&h, "pi", "Vec<i32>")
            .await
            .unwrap(),
        vec![1, 2, 3]
    );
    let scanned = conn
        .hscan_and_deserialize::<Vec<i32>>(&h, "p*", None)
        .await
        .unwrap();
    assert_eq!(scanned, vec![vec![1, 2, 3]]);

    let ka = key("ha");
    let kb = key("hb");
    let replies = conn
        .serialize_and_set_multiple_hash_field_if_not_exist(&[(&ka, 1_u8), (&kb, 2_u8)], "n", None)
        .await
        .unwrap();
    assert_eq!(replies, vec![HsetnxReply::KeySet, HsetnxReply::KeySet]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn hincrby_is_atomic() {
    let Some(conn) = connect("hincr").await else {
        return;
    };
    let h = key("counter");
    let mut handles = Vec::new();
    for _ in 0..20 {
        let conn = conn.clone();
        handles.push(tokio::spawn(async move {
            conn.increment_fields_in_hash(&key("counter"), &[("count", 1)])
                .await
                .unwrap()
        }));
    }
    let mut seen = Vec::new();
    for handle in handles {
        seen.extend(handle.await.unwrap());
    }
    seen.sort_unstable();
    assert_eq!(
        seen,
        (1..=20).collect::<Vec<usize>>(),
        "every increment observed a distinct value"
    );
    assert_eq!(conn.get_hash_field::<i64>(&h, "count").await.unwrap(), 20);

    let two = conn
        .increment_fields_in_hash(&h, &[("a", 5), ("b", 7)])
        .await
        .unwrap();
    assert_eq!(two, vec![5, 7]);
}

#[tokio::test]
async fn sadd_reports_new_members() {
    let Some(conn) = connect("sadd").await else {
        return;
    };
    let s = key("set");
    assert_eq!(
        conn.sadd(&s, vec!["a", "b"]).await.unwrap(),
        SaddReply::KeySet(2)
    );
    assert_eq!(conn.sadd(&s, "b").await.unwrap(), SaddReply::KeyNotSet);
    assert_eq!(conn.sadd(&s, "b").await.unwrap(), SaddReply::KeyNotSet);
    assert_eq!(conn.sadd(&s, "c").await.unwrap(), SaddReply::KeySet(1));
}

#[tokio::test]
async fn set_if_not_exists_and_get_value() {
    let Some(conn) = connect("setget").await else {
        return;
    };
    let k = key("k");
    match conn
        .set_key_if_not_exists_and_get_value::<String>(&k, "first".to_string(), Some(30))
        .await
        .unwrap()
    {
        SetGetReply::ValueSet(v) => assert_eq!(v, "first"),
        SetGetReply::ValueExists(_) => panic!("should have been set"),
    }
    match conn
        .set_key_if_not_exists_and_get_value::<String>(&k, "second".to_string(), Some(30))
        .await
        .unwrap()
    {
        SetGetReply::ValueExists(v) => assert_eq!(v, "first"),
        SetGetReply::ValueSet(_) => panic!("must not overwrite"),
    }
}

#[tokio::test]
async fn tenant_prefix_isolates_keys() {
    let Some(url) = test_url() else {
        return;
    };
    let pool = RedisConnectionPool::new_without_event_emitter(&settings(&url))
        .await
        .unwrap();
    let suffix = std::process::id();
    let a = RedisConnectionWithContext::new_without_context(Arc::new(
        pool.clone(&format!("tenant_a_{suffix}")),
    ));
    let b = RedisConnectionWithContext::new_without_context(Arc::new(
        pool.clone(&format!("tenant_b_{suffix}")),
    ));
    a.set_key(&key("shared"), "from-a").await.unwrap();
    assert!(b
        .get_key::<Option<String>>(&key("shared"))
        .await
        .unwrap()
        .is_none());
    b.set_key(&key("shared"), "from-b").await.unwrap();
    assert_eq!(a.get_key::<String>(&key("shared")).await.unwrap(), "from-a");
    assert_eq!(b.get_key::<String>(&key("shared")).await.unwrap(), "from-b");
    a.delete_key(&key("shared")).await.unwrap();
    b.delete_key(&key("shared")).await.unwrap();
}

#[tokio::test]
async fn pubsub_delivers_to_subscribed_channels_only() {
    let Some(url) = test_url() else {
        return;
    };
    let pool = RedisConnectionPool::new_without_event_emitter(&settings(&url))
        .await
        .unwrap();
    let channel = format!("chan_{}", std::process::id());
    let other = format!("other_{}", std::process::id());

    pool.subscriber.subscribe(&channel).await.unwrap();
    let mut rx = pool.subscriber.message_rx();

    pool.publisher
        .publish(&other, RedisValue::from_string("ignored".into()))
        .await
        .unwrap();
    pool.publisher
        .publish(&channel, RedisValue::from_string("hello".into()))
        .await
        .unwrap();
    pool.publisher
        .publish(&channel, RedisValue::from_bytes(b"second".to_vec()))
        .await
        .unwrap();

    let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("message within 5s")
        .unwrap();
    assert_eq!(first.channel, channel);
    assert_eq!(first.value.as_string().as_deref(), Some("hello"));
    let second = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("message within 5s")
        .unwrap();
    assert_eq!(second.value.as_bytes(), Some(&b"second"[..]));

    pool.subscriber.unsubscribe(&channel).await.unwrap();
    pool.publisher
        .publish(&channel, RedisValue::from_string("late".into()))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(900), rx.recv())
            .await
            .is_err(),
        "no delivery after unsubscribe"
    );
}

#[tokio::test]
async fn two_instances_see_each_others_messages() {
    let Some(url) = test_url() else {
        return;
    };
    let one = RedisConnectionPool::new_without_event_emitter(&settings(&url))
        .await
        .unwrap();
    let two = RedisConnectionPool::new_without_event_emitter(&settings(&url))
        .await
        .unwrap();
    let channel = format!("xinst_{}", std::process::id());
    two.subscriber.subscribe(&channel).await.unwrap();
    let mut rx = two.subscriber.message_rx();
    one.publisher
        .publish(&channel, RedisValue::from_string("invalidate".into()))
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("cross-instance delivery within 5s")
        .unwrap();
    assert_eq!(message.value.as_string().as_deref(), Some("invalidate"));
}

#[tokio::test]
async fn streams_fail_loudly_instead_of_silently() {
    let Some(conn) = connect("streams").await else {
        return;
    };
    let err = conn
        .stream_append_entry(&key("s"), &RedisEntryId::AutoGeneratedID, vec![("a", "b")])
        .await
        .unwrap_err();
    assert_eq!(*err.current_context(), RedisError::StreamAppendFailed);
    assert!(conn
        .consumer_group_create(&key("s"), "g", &RedisEntryId::AfterLastID)
        .await
        .is_err());
}

#[tokio::test]
async fn connection_errors_are_reported_not_panicked() {
    // no url configured
    let err = RedisConnectionPool::new_without_event_emitter(&RedisSettings::default())
        .await
        .err()
        .expect("must fail");
    assert!(matches!(
        err.current_context(),
        RedisError::InvalidConfiguration(_)
    ));
    assert!(RedisSettings::default().validate().is_err());

    // unreachable server
    let unreachable = RedisSettings {
        postgres_url: PostgresUrl::new("postgres://nobody:nothing@127.0.0.1:1/none"),
        default_command_timeout: 2,
        ..RedisSettings::default()
    };
    assert!(RedisConnectionPool::new_without_event_emitter(&unreachable)
        .await
        .is_err());
}

#[test]
fn postgres_url_never_leaks_through_debug() {
    let settings = RedisSettings {
        postgres_url: PostgresUrl::new("postgres://user:hunter2@host/db"),
        ..RedisSettings::default()
    };
    let rendered = format!("{settings:?}");
    assert!(!rendered.contains("hunter2"), "{rendered}");
    assert!(rendered.contains("redacted"));
    assert!(settings.validate().is_ok());
}
