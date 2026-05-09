/* global window */
// Fake but realistic data for the Wardnet prototype.

const WardData = (() => {
  const devices = [
    { name: "MacBook Pro Pedro", mac: "16:a8:f2:7b:98:5e", ip: "192.168.100.10", type: "laptop", vendor: "Apple", os: "macOS 15.2", up: 1820, down: 245, status: "online", group: "Trusted" },
    { name: "Galaxy S24 Leilah", mac: "5a:14:a4:4d:40:a9", ip: "192.168.100.52", type: "phone", vendor: "Samsung", os: "Android 15", up: 90, down: 320, status: "online", group: "Trusted" },
    { name: "Galaxy S24 Pedro", mac: "f2:6c:06:dc:b2:9c", ip: "192.168.100.55", type: "phone", vendor: "Samsung", os: "Android 15", up: 41, down: 86, status: "online", group: "Trusted" },
    { name: "Galaxy S21 Leilah", mac: "ba:d3:47:ca:d4:73", ip: "192.168.100.61", type: "phone", vendor: "Samsung", os: "Android 14", up: 12, down: 60, status: "idle", group: "Trusted" },
    { name: "Samsung TV Living Room", mac: "1c:af:4a:13:9c:ee", ip: "192.168.100.60", type: "tv", vendor: "Samsung", os: "Tizen 8", up: 2, down: 1820, status: "online", group: "Media" },
    { name: "MiBox Living Room", mac: "a4:e8:8d:cf:d7:10", ip: "192.168.100.62", type: "tv", vendor: "Xiaomi", os: "Android TV 12", up: 4, down: 412, status: "online", group: "Media" },
    { name: "XBOX", mac: "90:6a:eb:aa:c3:0e", ip: "192.168.100.53", type: "console", vendor: "Microsoft", os: "Xbox OS", up: 80, down: 880, status: "online", group: "Media" },
    { name: "TL-WPA7517", mac: "8c:86:dd:3d:0f:96", ip: "192.168.100.50", type: "ap", vendor: "TP-Link", os: "—", up: 1, down: 0, status: "online", group: "Infrastructure" },
    { name: "Linksys Main Router", mac: "d8:ec:5e:8f:3b:e1", ip: "192.168.100.4", type: "router", vendor: "Linksys", os: "OpenWrt 24.10", up: 220, down: 220, status: "online", group: "Infrastructure" },
    { name: "Linksys 06193", mac: "d8:ec:5e:8f:3c:95", ip: "192.168.100.58", type: "ap", vendor: "Linksys", os: "—", up: 0, down: 0, status: "idle", group: "Infrastructure" },
    { name: "Linksys 06196", mac: "d8:ec:5e:8f:3c:a4", ip: "192.168.100.59", type: "ap", vendor: "Linksys", os: "—", up: 0, down: 0, status: "idle", group: "Infrastructure" },
    { name: "Linksys 00479", mac: "80:69:1a:75:e1:58", ip: "192.168.100.51", type: "ap", vendor: "Linksys", os: "—", up: 0, down: 0, status: "idle", group: "Infrastructure" },
    { name: "Office Printer", mac: "30:cd:a7:11:b0:42", ip: "192.168.100.71", type: "printer", vendor: "Brother", os: "—", up: 0, down: 0, status: "idle", group: "Office" },
    { name: "Hue Bridge", mac: "ec:b5:fa:4e:cc:11", ip: "192.168.100.80", type: "iot", vendor: "Philips", os: "—", up: 1, down: 1, status: "online", group: "IoT" },
    { name: "Sonos Beam", mac: "e0:3e:cb:0b:81:a4", ip: "192.168.100.54", type: "iot", vendor: "Sonos", os: "—", up: 8, down: 12, status: "online", group: "Media" },
    { name: "Unknown · 5c:e7:53", mac: "5c:e7:53:4e:ec:db", ip: "192.168.100.56", type: "unknown", vendor: "—", os: "—", up: 0, down: 0, status: "idle", group: "Unclassified" },
  ];

  const tunnels = [
    { name: "Portugal Dedicated IP", country: "PT", flag: "🇵🇹", provider: "NordVPN", iface: "wg_ward0", endpoint: "pt131.nordvpn.com:51820", up: 32.5, upUnit: "MB", down: 282.1, downUnit: "MB", handshake: "just now", status: "active", load: 0.74 },
    { name: "Portugal", country: "PT", flag: "🇵🇹", provider: "NordVPN", iface: "wg_ward1", endpoint: "pt121.nordvpn.com:51820", up: 397.4, upUnit: "KB", down: 1.1, downUnit: "MB", handshake: "1m ago", status: "active", load: 0.18 },
    { name: "United States", country: "US", flag: "🇺🇸", provider: "NordVPN", iface: "wg_ward2", endpoint: "us5955.nordvpn.com:51820", up: 123.8, upUnit: "KB", down: 687.8, downUnit: "KB", handshake: "22d ago", status: "down", load: 0 },
  ];

  const filteringCategories = [
    { id: "ads", name: "Advertising", desc: "Trackers, ad networks, telemetry", count: 1247, on: true },
    { id: "malware", name: "Malware & phishing", desc: "Known malicious hosts", count: 92, on: true },
    { id: "adult", name: "Adult content", desc: "NSFW domains", count: 18, on: false },
    { id: "social", name: "Social media", desc: "X, Facebook, TikTok…", count: 6, on: false },
    { id: "gambling", name: "Gambling", desc: "Casinos, sports books", count: 41, on: true },
    { id: "crypto", name: "Crypto miners", desc: "In-browser miners", count: 12, on: true },
  ];

  const topDomains = [
    { d: "graph.facebook.com", q: 18421, blocked: false },
    { d: "doubleclick.net", q: 14210, blocked: true },
    { d: "googleapis.com", q: 13880, blocked: false },
    { d: "samsungcloud.com", q: 9240, blocked: false },
    { d: "telemetry.microsoft.com", q: 7821, blocked: true },
    { d: "icloud.com", q: 6530, blocked: false },
    { d: "ssl.gstatic.com", q: 6122, blocked: false },
    { d: "amplitude.com", q: 4108, blocked: true },
  ];

  const sparkA = "8 9 12 11 10 14 18 16 12 13 17 22 19 20 18 22 24 21 19 18 16 14 12 11 12 14 13";
  const sparkB = "20 22 19 21 23 26 28 31 29 27 30 28 26 24 22 23 25 27 29 30 28 26 25 24 22 21 23";
  const sparkC = "4 6 5 7 8 6 5 4 4 5 6 7 8 9 8 7 6 5 4 4 3 4 5 6 5 4 5";

  const dnsBars = Array.from({ length: 48 }, (_, i) => {
    const base = 30 + 30 * Math.sin(i / 6) + 25 * Math.cos(i / 3.2) + 20;
    const ok = Math.max(8, Math.round(base + (i % 5) * 4));
    const blk = Math.max(0, Math.round(ok * (0.02 + (i % 7) * 0.005)));
    return { ok, blk };
  });

  const initialLogs = [
    { t: "12:27:41.820", lvl: "warn", m: "Specified IFLA_INET6_ICMP6STATS NLA attribute holds more (most likely new kernel) data which is unknown to netlink-packet-route crate, expecting 48, got 56" },
    { t: "12:27:41.820", lvl: "warn", m: "Specified IFLA_INET6_STATS NLA attribute holds more (most likely new kernel) data which is unknown to netlink-packet-route crate, expecting 288, got 304" },
    { t: "12:27:39.402", lvl: "info", m: "DHCP lease assigned 192.168.100.62 → MiBox Living Room (a4:e8:8d:cf:d7:10) for 86400s" },
    { t: "12:27:35.118", lvl: "info", m: "wg_ward0 handshake with pt131.nordvpn.com:51820 (rx 12.4 KB, tx 8.1 KB)" },
    { t: "12:27:21.001", lvl: "info", m: "DNS query A graph.facebook.com → 157.240.221.35 client=192.168.100.55" },
    { t: "12:27:19.447", lvl: "info", m: "DNS query A doubleclick.net → BLOCKED (advertising) client=192.168.100.10" },
    { t: "12:27:11.880", lvl: "warn", m: "wg_ward2: peer handshake hasn't completed in 22d, marking interface down" },
    { t: "12:26:58.102", lvl: "info", m: "Device joined network: Galaxy S24 Leilah (5a:14:a4:4d:40:a9)" },
    { t: "12:26:42.778", lvl: "err",  m: "Failed to resolve telemetry.microsoft.com after 3 retries; upstream timeout" },
    { t: "12:26:31.101", lvl: "info", m: "Firewall rule applied: allow 192.168.100.4/32 → wg_ward1" },
  ];

  return { devices, tunnels, filteringCategories, topDomains, sparkA, sparkB, sparkC, dnsBars, initialLogs };
})();

window.WardData = WardData;
