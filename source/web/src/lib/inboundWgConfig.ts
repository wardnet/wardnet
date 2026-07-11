/**
 * Client-side helpers for inbound-WireGuard peer grants (issues #809-#813).
 *
 * The WireGuard client `.conf` itself is assembled daemon-side and returned by
 * `addPeer` as `client_config` (the only place the peer's private key is ever
 * exposed, admin-gated). The frontend just renders that string as a QR code or
 * downloads it, so the only helper needed here is a safe download filename.
 */

/**
 * Filesystem-safe base filename (no extension) for a peer's `.conf`, derived
 * from the device name. Collapses whitespace to `-`, drops characters that
 * are hostile in a filename (slashes, dots that could form path segments,
 * etc.), and falls back to `peer` when nothing usable remains, so a blank
 * or all-symbol device name never produces a hidden/`.conf`-only download.
 */
export function peerConfigFilename(name: string): string {
  const safe = name
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9_-]/g, "");
  return safe || "peer";
}
