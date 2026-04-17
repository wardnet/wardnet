use std::net::SocketAddr;

use async_trait::async_trait;

use wardnetd_services::device::hostname_resolver::HostnameResolver;

/// Real hostname resolver using system DNS.
///
/// Uses `getent hosts` for reverse DNS lookup. On Linux, falls back to
/// `avahi-resolve` for mDNS hostnames if reverse DNS fails.
pub struct SystemHostnameResolver;

#[async_trait]
impl HostnameResolver for SystemHostnameResolver {
    async fn resolve(&self, ip: &str) -> Option<String> {
        // Use spawn_blocking for the blocking DNS reverse lookup.
        let ip_str = ip.to_owned();
        let rdns_result = tokio::task::spawn_blocking(move || {
            // getnameinfo equivalent: resolve (ip, 0) back to a hostname
            let sock_addr: SocketAddr = format!("{ip_str}:0").parse().ok()?;
            dns_lookup_reverse(sock_addr)
        })
        .await
        .ok()?;

        if let Some(hostname) = rdns_result {
            return Some(hostname);
        }

        // On Linux, try avahi-resolve for mDNS hostnames.
        #[cfg(target_os = "linux")]
        {
            if let Some(hostname) = avahi_resolve(ip).await {
                return Some(hostname);
            }
        }

        None
    }
}

/// Perform a reverse DNS lookup using the system resolver.
fn dns_lookup_reverse(addr: SocketAddr) -> Option<String> {
    // Use libc-level getnameinfo via std's internal resolver.
    // std doesn't expose getnameinfo directly, so we rely on the
    // dns-lookup crate pattern: convert IP to in-addr.arpa and query.
    // For simplicity, use the `dns_lookup` approach via gethostbyaddr.
    //
    // Actually, we can use std::net::ToSocketAddrs in reverse by checking
    // if the system supports it. The simplest portable approach is to
    // shell out to `getent hosts <ip>` or use libc directly.
    //
    // We use a simple approach: try to resolve via the `host` command or
    // fall back to checking /etc/hosts.
    let ip = addr.ip();
    let output = std::process::Command::new("getent")
        .args(["hosts", &ip.to_string()])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // getent hosts output: "<ip>  <hostname> [aliases...]"
    let hostname = stdout.split_whitespace().nth(1)?;

    // Skip if the "hostname" is just the IP address repeated
    if hostname == ip.to_string() {
        return None;
    }

    Some(hostname.trim_end_matches('.').to_owned())
}

/// Try to resolve a hostname via Avahi mDNS (Linux only).
#[cfg(target_os = "linux")]
async fn avahi_resolve(ip: &str) -> Option<String> {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::process::Command::new("avahi-resolve")
            .args(["-a", ip])
            .output(),
    )
    .await
    .ok()?
    .ok()?;

    if !result.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&result.stdout);
    // avahi-resolve -a output: "<ip>\t<hostname>"
    let hostname = stdout.split('\t').nth(1)?.trim();
    if hostname.is_empty() {
        return None;
    }

    Some(hostname.trim_end_matches('.').to_owned())
}
