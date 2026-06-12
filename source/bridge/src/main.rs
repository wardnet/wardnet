use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use wardnet_bridge::{
    api,
    cloudflare::CloudflareDnsProvider,
    config::Config,
    db, http01,
    repository::{
        PgChallengeRepository, PgInstallRepository, PgNameRepository, PgTlsRepository,
        TlsRepository,
    },
    sni::{self, Role},
    state::AppState,
    sweep,
    tls::{self, CertResolver, TlsRenewalRunner},
    tunnel::TunnelRegistry,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().json())
        .with(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env()?;

    // rustls 0.23 needs an explicit default crypto provider before any TLS work.
    tls::install_crypto_provider();

    tracing::info!(
        region = %config.region,
        fqdn = %config.fqdn,
        subdomain_parent = %config.subdomain_parent,
        http01_listen_addr = %config.http01_listen_addr,
        tls_listen_addr = %config.tls_listen_addr,
        dot_listen_addr = %config.dot_listen_addr,
        "wardnet-bridge starting"
    );

    let pools = db::init(&config.database_url).await?;
    let global_pools = db::init_global(&config.global_database_url).await?;

    let installs = Arc::new(PgInstallRepository::new_pools(pools.clone()));
    let names = Arc::new(PgNameRepository::new_pools(global_pools));
    let challenges = Arc::new(PgChallengeRepository::new_pools(pools.clone()));
    let tls_repo: Arc<dyn TlsRepository> = Arc::new(PgTlsRepository::new_pools(pools.clone()));
    let dns = Arc::new(CloudflareDnsProvider::new(
        &config.cloudflare_api_token,
        &config.cloudflare_zone_id,
    )?);
    let tunnel_registry = Arc::new(TunnelRegistry::new());

    // Reap abandoned name reservations (a two-database saga) and expired ACME
    // HTTP-01 challenge tokens for this region.
    tokio::spawn(sweep::run(
        Arc::clone(&names) as Arc<dyn wardnet_bridge::repository::NameRepository>,
        Arc::clone(&installs) as Arc<dyn wardnet_bridge::repository::InstallRepository>,
        Arc::clone(&tls_repo),
        config.region.clone(),
    ));

    let state = AppState::new(
        config.clone(),
        pools,
        installs,
        names,
        challenges,
        dns,
        Arc::clone(&tunnel_registry),
    );
    let api_router = api::router(state);

    // The :8443 listener serves the API over a cert that starts as a placeholder
    // and is hot-swapped to the real one by the renewal runner. The same resolver
    // is shared by both so a swap is immediately visible to new connections.
    let resolver = CertResolver::with_placeholder()?;

    let runner = TlsRenewalRunner::new(
        Arc::clone(&tls_repo),
        Arc::clone(&resolver),
        config.fqdn.clone(),
        config.acme_directory_url.clone(),
        config.encryption_key,
    );
    tokio::spawn(runner.run());

    let fqdn: Arc<str> = Arc::from(config.fqdn.as_str());

    tokio::select! {
        res = http01::run(&config.http01_listen_addr, Arc::clone(&tls_repo)) => res?,

        res = sni::run(
            &config.tls_listen_addr,
            Arc::clone(&fqdn),
            &config.subdomain_parent,
            Arc::clone(&tunnel_registry),
            Role::Https { resolver: Arc::clone(&resolver), api_router },
        ) => res?,

        res = sni::run(
            &config.dot_listen_addr,
            Arc::clone(&fqdn),
            &config.subdomain_parent,
            Arc::clone(&tunnel_registry),
            Role::Dot,
        ) => res?,
    }

    Ok(())
}
