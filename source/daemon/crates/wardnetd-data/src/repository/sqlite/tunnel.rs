use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::tunnel::{BestServerSelector, Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;

use super::super::TunnelRepository;
use super::super::tunnel::TunnelRow;
use crate::db::DbPools;

/// SQLite-backed implementation of [`TunnelRepository`].
pub struct SqliteTunnelRepository {
    pools: DbPools,
}

impl SqliteTunnelRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

/// Raw row from the `tunnels` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DbTunnelRow {
    id: String,
    label: String,
    country_code: String,
    provider: Option<String>,
    interface_name: String,
    endpoint: String,
    status: String,
    last_handshake: Option<String>,
    bytes_tx: i64,
    bytes_rx: i64,
    created_at: String,
    override_default_dns: i64,
    server_selector_country: Option<String>,
    resolved_server_name: Option<String>,
    endpoint_resolved_at: Option<String>,
}

impl DbTunnelRow {
    /// Convert the raw database row into a domain [`Tunnel`].
    fn into_tunnel(self) -> anyhow::Result<Tunnel> {
        let status = match self.status.as_str() {
            "up" => TunnelStatus::Up,
            "connecting" => TunnelStatus::Connecting,
            "reconnecting" => TunnelStatus::Reconnecting,
            _ => TunnelStatus::Down,
        };

        let last_handshake = self.last_handshake.as_deref().map(str::parse).transpose()?;
        let endpoint_resolved_at = self
            .endpoint_resolved_at
            .as_deref()
            .map(str::parse)
            .transpose()?;
        let server_selector = self
            .server_selector_country
            .map(|country| BestServerSelector { country });

        Ok(Tunnel {
            id: self.id.parse()?,
            label: self.label,
            country_code: self.country_code,
            provider: self.provider,
            interface_name: self.interface_name,
            endpoint: self.endpoint,
            status,
            last_handshake,
            bytes_tx: self.bytes_tx.cast_unsigned(),
            bytes_rx: self.bytes_rx.cast_unsigned(),
            created_at: self.created_at.parse()?,
            override_default_dns: self.override_default_dns != 0,
            server_selector,
            resolved_server_name: self.resolved_server_name,
            endpoint_resolved_at,
        })
    }
}

/// Raw row for the `WireGuard` configuration columns of a tunnel.
#[derive(sqlx::FromRow)]
struct DbTunnelConfigRow {
    address: String,
    dns: String,
    peer_config: String,
    listen_port: Option<i64>,
    override_default_dns: i64,
}

impl DbTunnelConfigRow {
    /// Deserialize the raw JSON columns into a domain [`TunnelConfig`].
    fn into_tunnel_config(self) -> anyhow::Result<TunnelConfig> {
        let address: Vec<String> = serde_json::from_str(&self.address)?;
        let dns: Vec<String> = serde_json::from_str(&self.dns)?;
        let peer: WgPeerConfig = serde_json::from_str(&self.peer_config)?;
        let listen_port = self
            .listen_port
            .map(u16::try_from)
            .transpose()
            .map_err(|e| anyhow::anyhow!("invalid listen_port value: {e}"))?;
        Ok(TunnelConfig {
            address,
            dns,
            listen_port,
            peer,
            override_default_dns: self.override_default_dns != 0,
        })
    }
}

#[async_trait]
impl TunnelRepository for SqliteTunnelRepository {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        let rows = sqlx::query_as::<_, DbTunnelRow>(
            "SELECT id, label, country_code, provider, interface_name, endpoint, \
             status, last_handshake, bytes_tx, bytes_rx, created_at, override_default_dns, \
             server_selector_country, resolved_server_name, endpoint_resolved_at \
             FROM tunnels",
        )
        .fetch_all(&self.pools.read)
        .await?;

        rows.into_iter().map(DbTunnelRow::into_tunnel).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        let row = sqlx::query_as::<_, DbTunnelRow>(
            "SELECT id, label, country_code, provider, interface_name, endpoint, \
             status, last_handshake, bytes_tx, bytes_rx, created_at, override_default_dns, \
             server_selector_country, resolved_server_name, endpoint_resolved_at \
             FROM tunnels WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pools.read)
        .await?;

        row.map(DbTunnelRow::into_tunnel).transpose()
    }

    async fn find_config_by_id(&self, id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        let row = sqlx::query_as::<_, DbTunnelConfigRow>(
            "SELECT address, dns, peer_config, listen_port, override_default_dns \
             FROM tunnels WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pools.read)
        .await?;

        row.map(DbTunnelConfigRow::into_tunnel_config).transpose()
    }

    async fn insert(&self, row: &TunnelRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tunnels (id, label, country_code, provider, interface_name, \
             endpoint, status, address, dns, peer_config, listen_port, override_default_dns, \
             server_selector_country, resolved_server_name, endpoint_resolved_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.label)
        .bind(&row.country_code)
        .bind(&row.provider)
        .bind(&row.interface_name)
        .bind(&row.endpoint)
        .bind(&row.status)
        .bind(&row.address)
        .bind(&row.dns)
        .bind(&row.peer_config)
        .bind(row.listen_port)
        .bind(i64::from(row.override_default_dns))
        .bind(&row.server_selector_country)
        .bind(&row.resolved_server_name)
        .bind(&row.endpoint_resolved_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_endpoint(
        &self,
        id: &str,
        endpoint: &str,
        peer_config_json: &str,
        server_name: &str,
        resolved_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE tunnels \
             SET endpoint = ?, peer_config = ?, \
                 resolved_server_name = ?, endpoint_resolved_at = ? \
             WHERE id = ?",
        )
        .bind(endpoint)
        .bind(peer_config_json)
        .bind(server_name)
        .bind(resolved_at)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_status(&self, id: &str, status: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE tunnels SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn update_dns_override(&self, id: &str, value: bool) -> anyhow::Result<()> {
        sqlx::query("UPDATE tunnels SET override_default_dns = ? WHERE id = ?")
            .bind(i64::from(value))
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn update_stats(
        &self,
        id: &str,
        bytes_tx: i64,
        bytes_rx: i64,
        last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE tunnels SET bytes_tx = ?, bytes_rx = ?, last_handshake = ? WHERE id = ?",
        )
        .bind(bytes_tx)
        .bind(bytes_rx)
        .bind(last_handshake)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tunnels WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        let idx = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(CAST(REPLACE(interface_name, 'wg_ward', '') AS INTEGER)) + 1, 0) \
             FROM tunnels",
        )
        .fetch_one(&self.pools.read)
        .await?;
        Ok(idx)
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels")
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn count_active(&self) -> anyhow::Result<i64> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels WHERE status = 'up'")
                .fetch_one(&self.pools.read)
                .await?;
        Ok(count)
    }
}
