-- Per-tunnel flag: when true, devices targeting this tunnel route their
-- DNS queries through wardnet (so the ad-blocking filter still runs), and
-- wardnet uses the tunnel's DNS server as the upstream for those queries
-- with SO_BINDTODEVICE so the upstream packets egress via the tunnel
-- interface (no leakage to the ISP).
--
-- Replaces the previous nftables prerouting DNAT mechanism, which silently
-- bypassed the ad-blocking filter for tunneled devices (issue #342).
--
-- Backfill: every existing tunnel that has a configured DNS server keeps
-- the same leak-prevention behaviour post-upgrade — `override_default_dns`
-- defaults to true for those rows. Tunnels with an empty `dns` array
-- (`'[]'`) get false, which is the no-op for routing.
ALTER TABLE tunnels ADD COLUMN override_default_dns INTEGER NOT NULL DEFAULT 0;

UPDATE tunnels
SET    override_default_dns = 1
WHERE  dns IS NOT NULL
  AND  TRIM(dns) <> ''
  AND  TRIM(dns) <> '[]';
