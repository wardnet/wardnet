use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_types::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_types::wireguard_config::WgPeerConfig;

use super::super::TunnelRepository;
use super::super::tunnel::TunnelRow;

/// SQLite-backed implementation of [`TunnelRepository`].
pub struct SqliteTunnelRepository {
    pool: SqlitePool,
}

impl SqliteTunnelRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
}

impl DbTunnelRow {
    /// Convert the raw database row into a domain [`Tunnel`].
    fn into_tunnel(self) -> anyhow::Result<Tunnel> {
        let status = match self.status.as_str() {
            "up" => TunnelStatus::Up,
            "connecting" => TunnelStatus::Connecting,
            _ => TunnelStatus::Down,
        };

        let last_handshake = self.last_handshake.as_deref().map(str::parse).transpose()?;

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
        })
    }
}

#[async_trait]
impl TunnelRepository for SqliteTunnelRepository {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        let rows = sqlx::query_as::<_, DbTunnelRow>(
            "SELECT id, label, country_code, provider, interface_name, endpoint, \
             status, last_handshake, bytes_tx, bytes_rx, created_at \
             FROM tunnels",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(DbTunnelRow::into_tunnel).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        let row = sqlx::query_as::<_, DbTunnelRow>(
            "SELECT id, label, country_code, provider, interface_name, endpoint, \
             status, last_handshake, bytes_tx, bytes_rx, created_at \
             FROM tunnels WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbTunnelRow::into_tunnel).transpose()
    }

    async fn find_config_by_id(&self, id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        let row = sqlx::query_as::<_, DbTunnelConfigRow>(
            "SELECT address, dns, peer_config, listen_port FROM tunnels WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(DbTunnelConfigRow::into_tunnel_config).transpose()
    }

    async fn insert(&self, row: &TunnelRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tunnels (id, label, country_code, provider, interface_name, \
             endpoint, status, address, dns, peer_config, listen_port) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, id: &str, status: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE tunnels SET status = ? WHERE id = ?")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM tunnels WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        let idx = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(MAX(CAST(REPLACE(interface_name, 'wg_ward', '') AS INTEGER)) + 1, 0) \
             FROM tunnels",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(idx)
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    async fn count_active(&self) -> anyhow::Result<i64> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels WHERE status = 'up'")
                .fetch_one(&self.pool)
                .await?;
        Ok(count)
    }
}
