use std::{future::Future, pin::Pin, sync::Arc};

use async_bb8_diesel::{AsyncConnection, ConnectionError};
use bb8::CustomizeConnection;
use common_utils::{
    external_service::{ExternalServiceEventEmitter, NoOpEventEmitter},
    types::{keymanager, TenantConfig},
    DbConnectionParams,
};
pub use diesel_models::DatabaseConnectionWithContext;
use diesel_models::{DejaPgConnection, RawPgConnection};
use error_stack::ResultExt;

use crate::{
    config::Database,
    errors::{StorageError, StorageResult},
};

pub type RawPgPool = bb8::Pool<async_bb8_diesel::ConnectionManager<DejaPgConnection>>;

#[derive(Debug, Clone)]
pub struct PgPool {
    pub pg_pool: RawPgPool,
    pub event_emitter: Arc<dyn ExternalServiceEventEmitter>,
}

impl PgPool {
    pub fn new(pg_pool: RawPgPool, event_emitter: Arc<dyn ExternalServiceEventEmitter>) -> Self {
        Self {
            pg_pool,
            event_emitter,
        }
    }

    pub fn new_without_event_emitter(pool: RawPgPool) -> Self {
        Self::new(pool, Arc::new(NoOpEventEmitter))
    }
}

/// Shared database-pool ownership.
///
/// Request context is intentionally not part of this trait. Request-scoped wrappers such as
/// `RouterStore` and `KVRouterStore` implement `RequestContext` separately, and the connection
/// helpers in `crate::connection` combine the two when leasing a connection.
#[async_trait::async_trait]
pub trait DatabaseStore: Clone + Send + Sync {
    type Config: Send;
    async fn new(
        config: Self::Config,
        tenant_config: &dyn TenantConfig,
        test_transaction: bool,
        key_manager_state: Option<keymanager::KeyManagerState>,
        event_emitter: Arc<dyn ExternalServiceEventEmitter>,
    ) -> StorageResult<Self>;
    fn get_master_pool(&self) -> &PgPool;
    fn get_replica_pool(&self) -> &PgPool;
    fn get_accounts_master_pool(&self) -> &PgPool;
    fn get_accounts_replica_pool(&self) -> &PgPool;

    /// Request correlation used by deja replay to route database connections to
    /// the active replay schema. Stores without request identity return `None`.
    #[cfg(feature = "deja")]
    fn get_request_id(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct Store {
    pub master_pool: PgPool,
    pub accounts_pool: PgPool,
}

#[async_trait::async_trait]
impl DatabaseStore for Store {
    /// (master config, accounts config)
    type Config = (Database, Database);
    async fn new(
        config: (Database, Database),
        tenant_config: &dyn TenantConfig,
        test_transaction: bool,
        _key_manager_state: Option<keymanager::KeyManagerState>,
        event_emitter: Arc<dyn ExternalServiceEventEmitter>,
    ) -> StorageResult<Self> {
        let (master_config, accounts_config) = config;
        Ok(Self {
            master_pool: diesel_make_pg_pool(
                &master_config,
                tenant_config.get_schema(),
                test_transaction,
                Arc::clone(&event_emitter),
            )
            .await?,
            accounts_pool: diesel_make_pg_pool(
                &accounts_config,
                tenant_config.get_accounts_schema(),
                test_transaction,
                event_emitter,
            )
            .await?,
        })
    }

    fn get_master_pool(&self) -> &PgPool {
        &self.master_pool
    }

    fn get_replica_pool(&self) -> &PgPool {
        &self.master_pool
    }

    fn get_accounts_master_pool(&self) -> &PgPool {
        &self.accounts_pool
    }

    fn get_accounts_replica_pool(&self) -> &PgPool {
        &self.accounts_pool
    }
}

#[derive(Debug, Clone)]
pub struct ReplicaStore {
    pub master_pool: PgPool,
    pub replica_pool: PgPool,
    pub accounts_master_pool: PgPool,
    pub accounts_replica_pool: PgPool,
}

#[async_trait::async_trait]
impl DatabaseStore for ReplicaStore {
    /// (master config, replica config, accounts master config, accounts replica config)
    type Config = (Database, Database, Database, Database);
    async fn new(
        config: (Database, Database, Database, Database),
        tenant_config: &dyn TenantConfig,
        test_transaction: bool,
        _key_manager_state: Option<keymanager::KeyManagerState>,
        event_emitter: Arc<dyn ExternalServiceEventEmitter>,
    ) -> StorageResult<Self> {
        let (master_config, replica_config, accounts_master_config, accounts_replica_config) =
            config;
        let master_pool = diesel_make_pg_pool(
            &master_config,
            tenant_config.get_schema(),
            test_transaction,
            Arc::clone(&event_emitter),
        )
        .await
        .attach_printable("failed to create master pool")?;
        let accounts_master_pool = diesel_make_pg_pool(
            &accounts_master_config,
            tenant_config.get_accounts_schema(),
            test_transaction,
            Arc::clone(&event_emitter),
        )
        .await
        .attach_printable("failed to create accounts master pool")?;
        let replica_pool = diesel_make_pg_pool(
            &replica_config,
            tenant_config.get_schema(),
            test_transaction,
            Arc::clone(&event_emitter),
        )
        .await
        .attach_printable("failed to create replica pool")?;

        let accounts_replica_pool = diesel_make_pg_pool(
            &accounts_replica_config,
            tenant_config.get_accounts_schema(),
            test_transaction,
            event_emitter,
        )
        .await
        .attach_printable("failed to create accounts pool")?;
        Ok(Self {
            master_pool,
            replica_pool,
            accounts_master_pool,
            accounts_replica_pool,
        })
    }

    fn get_master_pool(&self) -> &PgPool {
        &self.master_pool
    }

    fn get_replica_pool(&self) -> &PgPool {
        &self.replica_pool
    }

    fn get_accounts_master_pool(&self) -> &PgPool {
        &self.accounts_master_pool
    }

    fn get_accounts_replica_pool(&self) -> &PgPool {
        &self.accounts_replica_pool
    }
}

pub async fn diesel_make_pg_pool(
    database: &Database,
    schema: &str,
    test_transaction: bool,
    event_emitter: Arc<dyn ExternalServiceEventEmitter>,
) -> StorageResult<PgPool> {
    let database_url = database.get_database_url(schema);
    let manager = async_bb8_diesel::ConnectionManager::<DejaPgConnection>::new(database_url);
    let mut pool = bb8::Pool::builder()
        .max_size(database.max_pool_size)
        .min_idle(Some(database.min_idle_pool_size))
        .queue_strategy(database.queue_strategy.into())
        .connection_timeout(std::time::Duration::from_secs(database.connection_timeout))
        .max_lifetime(std::time::Duration::from_secs(database.max_lifetime))
        .idle_timeout(std::time::Duration::from_secs(database.idle_timeout));

    // bb8 accepts exactly one customizer per pool, so every per-connection
    // setup step is composed into `ConnectionSetup`. Only install it when there
    // is something to do, so a default pool behaves exactly as it always has.
    let setup = ConnectionSetup {
        test_transaction,
        disable_prepared_statement_cache: database.disable_prepared_statement_cache,
    };
    if setup.is_needed() {
        if setup.disable_prepared_statement_cache {
            router_env::logger::info!(
                host = %database.host,
                port = database.port,
                "disabling the prepared-statement cache for this pool (transaction-mode pooler)"
            );
        }
        pool = pool.connection_customizer(Box::new(setup));
    }

    let raw_pool = pool
        .build(manager)
        .await
        .change_context(StorageError::InitializationError)
        .attach_printable("Failed to create PostgreSQL connection pool")?;

    // Register row identity (primary-key columns) with deja from this
    // database's own catalog. Idempotent; on failure identity stays
    // unregistered, making recorded row keys absent rather than wrong.
    #[cfg(feature = "deja")]
    if !deja::runtime_mode_is_disabled() {
        use async_bb8_diesel::AsyncConnection;
        use diesel::RunQueryDsl;
        if let Ok(connection) = raw_pool.get().await {
            let rows = connection
                .run(|conn| {
                    diesel::sql_query(deja::TABLE_IDENTITY_SQL)
                        .load::<deja::db::TableIdentityRow>(conn)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|row| (row.table_name, row.column_name))
                                .collect::<Vec<(String, String)>>()
                        })
                })
                .await;
            match rows {
                Ok(rows) => deja::db::register_table_identity_rows(rows),
                Err(error) => {
                    router_env::logger::warn!(
                        ?error,
                        "deja: could not read row identity from the schema; recorded row keys will fall back to query fingerprints"
                    );
                }
            }
        }
    }

    Ok(PgPool::new(raw_pool, event_emitter))
}

/// Per-connection setup, run by bb8 on every physical connection the pool
/// opens (including the `min_idle` ones created while the pool is built).
#[derive(Debug, Clone, Copy)]
struct ConnectionSetup {
    /// Wrap each connection in a transaction that is never committed (tests).
    test_transaction: bool,
    /// Stop diesel caching *named* prepared statements on the connection.
    ///
    /// Behind a transaction-mode pooler (Supabase Supavisor on port 6543,
    /// PgBouncer in transaction mode) the backend Postgres session changes
    /// between transactions, so a statement prepared earlier can be executed
    /// on a backend that has never seen it and fails with
    /// `prepared statement "..." does not exist`. With the cache disabled
    /// diesel uses unnamed statements, which is what such poolers support.
    disable_prepared_statement_cache: bool,
}

impl ConnectionSetup {
    const fn is_needed(self) -> bool {
        self.test_transaction || self.disable_prepared_statement_cache
    }
}

impl CustomizeConnection<RawPgConnection, ConnectionError> for ConnectionSetup {
    fn on_acquire<'a>(
        &'a self,
        conn: &'a mut RawPgConnection,
    ) -> Pin<Box<dyn Future<Output = Result<(), ConnectionError>> + Send + 'a>> {
        let Self {
            test_transaction,
            disable_prepared_statement_cache,
        } = *self;

        Box::pin(async move {
            use diesel::{connection::CacheSize, Connection};

            conn.run(move |conn| {
                // Must come first: it only affects statements prepared after it,
                // and `begin_test_transaction` issues one.
                if disable_prepared_statement_cache {
                    conn.set_prepared_statement_cache_size(CacheSize::Disabled);
                }
                if test_transaction {
                    #[allow(clippy::unwrap_used)]
                    conn.begin_test_transaction().unwrap();
                }
                Ok(())
            })
            .await
        })
    }
}
