//! Regression test for `Database::disable_prepared_statement_cache`.
//!
//! Behind a transaction-mode pooler (PgBouncer `pool_mode = transaction`,
//! Supabase Supavisor on port 6543) consecutive transactions of one client can
//! land on different backend Postgres sessions. Diesel caches *named* prepared
//! statements per connection, so it will happily re-use a statement name on a
//! backend that never prepared it and Postgres answers
//! `prepared statement "__diesel_stmt_N" does not exist`.
//!
//! This test drives the real pool builder (`diesel_make_pg_pool`) with the
//! flag on and off against a pooler you provide, so it is `#[ignore]`d and
//! only runs when asked for:
//!
//! ```text
//! BPAY_TEST_PG_HOST=127.0.0.1 BPAY_TEST_PG_PORT=6543 \
//! BPAY_TEST_PG_USER=bpay BPAY_TEST_PG_PASSWORD=bpay BPAY_TEST_PG_DBNAME=bpay \
//! cargo test -p storage_impl --test transaction_pooler -- --ignored --nocapture
//! ```
//!
//! The pooler must be in transaction mode, have protocol-level prepared
//! statement support off (PgBouncer `max_prepared_statements = 0`) and a
//! server pool smaller than the client concurrency used here (so backends are
//! genuinely shared). Set `BPAY_TEST_EXPECT_CACHE_ENABLED_TO_FAIL=1` to also
//! assert that the *control* run (flag off) really does hit the error - useful
//! to prove the pooler is configured so the test can fail at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use async_bb8_diesel::AsyncConnection;
use common_utils::external_service::NoOpEventEmitter;
use diesel::{
    sql_types::{Integer, Text},
    IntoSql,
};
use hyperswitch_masking::Secret;
use storage_impl::{config::Database, database::store::diesel_make_pg_pool};

const CLIENTS: usize = 8;
const ROUNDS: usize = 60;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set (see the module docs)"))
}

fn database(disable_prepared_statement_cache: bool) -> Database {
    Database {
        username: env("BPAY_TEST_PG_USER"),
        password: Secret::new(env("BPAY_TEST_PG_PASSWORD")),
        host: env("BPAY_TEST_PG_HOST"),
        port: env("BPAY_TEST_PG_PORT").parse().expect("numeric port"),
        dbname: env("BPAY_TEST_PG_DBNAME"),
        max_pool_size: CLIENTS as u32,
        min_idle_pool_size: 1,
        disable_prepared_statement_cache,
        ..Database::default()
    }
}

/// Runs `CLIENTS` concurrent tasks issuing several distinct statements with
/// statically-known query ids (the ones diesel caches by name) and returns the
/// `(successes, errors)` counts plus the first error seen.
async fn hammer(disable_prepared_statement_cache: bool) -> (usize, usize, Option<String>) {
    let pool = diesel_make_pg_pool(
        &database(disable_prepared_statement_cache),
        "public",
        false,
        Arc::new(NoOpEventEmitter),
    )
    .await
    .expect("pool should build");

    let ok = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let first_error = Arc::new(std::sync::Mutex::new(None::<String>));

    let mut tasks = Vec::new();
    for _ in 0..CLIENTS {
        let (pool, ok, failed, first_error) = (
            pool.clone(),
            Arc::clone(&ok),
            Arc::clone(&failed),
            Arc::clone(&first_error),
        );
        tasks.push(tokio::spawn(async move {
            for _ in 0..ROUNDS {
                let conn = pool.pg_pool.get().await.expect("pool checkout");
                let result = conn
                    .run(|conn| {
                        use diesel::RunQueryDsl;
                        diesel::select(1_i32.into_sql::<Integer>()).get_result::<i32>(conn)?;
                        diesel::select("a".into_sql::<Text>()).get_result::<String>(conn)?;
                        diesel::select((1_i32.into_sql::<Integer>(), 2_i32.into_sql::<Integer>()))
                            .get_result::<(i32, i32)>(conn)
                    })
                    .await;
                match result {
                    Ok(_) => ok.fetch_add(1, Ordering::Relaxed),
                    Err(error) => {
                        first_error
                            .lock()
                            .unwrap()
                            .get_or_insert_with(|| error.to_string());
                        failed.fetch_add(1, Ordering::Relaxed)
                    }
                };
            }
        }));
    }
    for task in tasks {
        task.await.expect("task panicked");
    }
    let first_error = first_error.lock().unwrap().clone();
    (
        ok.load(Ordering::Relaxed),
        failed.load(Ordering::Relaxed),
        first_error,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs a transaction-mode pooler; see module docs"]
async fn disabling_the_prepared_statement_cache_makes_a_transaction_pooler_safe() {
    let (ok, failed, first_error) = hammer(true).await;
    println!("cache DISABLED: ok={ok} failed={failed} first_error={first_error:?}");
    assert_eq!(
        failed, 0,
        "no statement may fail with the cache disabled; first error: {first_error:?}"
    );
    assert_eq!(ok, CLIENTS * ROUNDS);

    // Control run: the same load with the cache left on.
    let (ok, failed, first_error) = hammer(false).await;
    println!("cache ENABLED : ok={ok} failed={failed} first_error={first_error:?}");
    if std::env::var("BPAY_TEST_EXPECT_CACHE_ENABLED_TO_FAIL").as_deref() == Ok("1") {
        assert!(
            failed > 0,
            "control run should hit `prepared statement ... does not exist` behind a \
             transaction pooler; if it does not, the pooler is not configured as the test requires"
        );
    }
}
