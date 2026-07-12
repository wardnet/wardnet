-- Inbound (multi-peer) WireGuard server peers (issue #809).
--
-- The daemon can stand up a single inbound WireGuard *server* interface
-- (`wg_wardin0`) that many remote peers dial into — the mirror image of the
-- outbound single-peer `tunnels` subsystem. Each row is one admitted peer.
--
-- The peer's private key is NEVER persisted: it is generated on the daemon,
-- returned once in the add-peer API response, and then forgotten. Only the
-- public key and the allocated tunnel-subnet address are stored here, so a
-- database leak cannot reconstruct a peer's identity.
--
-- `allowed_ip` is the peer's fixed /32 inside the inbound tunnel subnet
-- (10.100.64.0/24); the server allocates it sequentially from .2 upward (.1 is
-- the server itself). It is UNIQUE so two peers can never share an address.
--
-- This table is intentionally standalone: peers are NOT wired into the
-- `devices` / routing / zone model (a separate future issue owns that), so
-- there are no foreign keys here.
CREATE TABLE IF NOT EXISTS inbound_wg_peers (
    id          TEXT PRIMARY KEY,            -- UUID
    public_key  TEXT NOT NULL UNIQUE,        -- base64 WireGuard public key
    allowed_ip  TEXT NOT NULL UNIQUE,        -- e.g. "10.100.64.2/32"
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,  -- 1 = admitted, 0 = disabled
    created_at  TEXT NOT NULL                -- RFC 3339 (lexicographically sortable)
);
