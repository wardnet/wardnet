use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wardnet_bridge::{
    api,
    cloudflare::CloudflareDnsProvider,
    config::Config,
    db,
    repository::{SqliteChallengeRepository, SqliteInstallRepository},
    state::AppState,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Structured JSON logging; RUST_LOG controls the filter.
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;

    tracing::info!(
        region = %config.region,
        subdomain_parent = %config.subdomain_parent,
        listen_addr = %config.listen_addr,
        "wardnet-bridge starting"
    );

    let pools = db::init(&config.database_url).await?;

    let installs = Arc::new(SqliteInstallRepository::new_pools(pools.clone()));
    let challenges = Arc::new(SqliteChallengeRepository::new_pools(pools.clone()));
    let dns = Arc::new(CloudflareDnsProvider::new(
        &config.cloudflare_api_token,
        &config.cloudflare_zone_id,
    )?);

    let state = AppState::new(config.clone(), pools, installs, challenges, dns);
    let router = api::router(state);

    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(addr = %listener.local_addr()?, "listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
