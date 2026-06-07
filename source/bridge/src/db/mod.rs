use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

/// Reader / writer pool pair backed by `PostgreSQL`.
///
/// Both `read` and `write` point at the same underlying connection pool;
/// the split is retained for API compatibility with the rest of the codebase
/// and to allow future reader replicas to be introduced without changing
/// call sites.
#[derive(Clone)]
pub struct DbPools {
    pub read: PgPool,
    pub write: PgPool,
}

impl DbPools {
    /// Wrap a single pool as both reader and writer.
    #[must_use]
    pub fn single(pool: PgPool) -> Self {
        Self {
            read: pool.clone(),
            write: pool,
        }
    }
}

/// Initialise the connection pool and run pending migrations.
pub async fn init(database_url: &str) -> anyhow::Result<DbPools> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database initialised");

    Ok(DbPools {
        read: pool.clone(),
        write: pool,
    })
}
