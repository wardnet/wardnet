use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wardnet_bridge::{
    api,
    cloudflare::CloudflareDnsProvider,
    config::Config,
    db,
    repository::{MySqlChallengeRepository, MySqlInstallRepository},
    sni,
    state::AppState,
    tunnel::TunnelRegistry,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;

    tracing::info!(
        region = %config.region,
        subdomain_parent = %config.subdomain_parent,
        bridge_hostname = %config.bridge_hostname,
        listen_addr = %config.listen_addr,
        sni_listen_addr = %config.sni_listen_addr,
        dot_listen_addr = %config.dot_listen_addr,
        "wardnet-bridge starting"
    );

    let pools = db::init(&config.database_url).await?;

    let installs = Arc::new(MySqlInstallRepository::new_pools(pools.clone()));
    let challenges = Arc::new(MySqlChallengeRepository::new_pools(pools.clone()));
    let dns = Arc::new(CloudflareDnsProvider::new(
        &config.cloudflare_api_token,
        &config.cloudflare_zone_id,
    )?);
    let tunnel_registry = Arc::new(TunnelRegistry::new());

    let state = AppState::new(
        config.clone(),
        pools,
        installs,
        challenges,
        dns,
        Arc::clone(&tunnel_registry),
    );

    let router = api::router(state);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(addr = %listener.local_addr()?, "HTTP API listening");

    let sni_config = config.clone();
    let dot_config = config.clone();
    let sni_reg = Arc::clone(&tunnel_registry);
    let dot_reg = Arc::clone(&tunnel_registry);

    tokio::select! {
        res = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        ) => { res?; }

        res = sni::run(
            &config.sni_listen_addr,
            443,
            sni_config,
            sni_reg,
        ) => { res?; }

        res = sni::run(
            &config.dot_listen_addr,
            853,
            dot_config,
            dot_reg,
        ) => { res?; }
    }

    Ok(())
}
