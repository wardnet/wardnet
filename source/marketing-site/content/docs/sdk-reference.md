# SDK reference

`@wardnet/js` is the TypeScript SDK that powers Wardnet's own admin
site, admin mobile PWA, and user PWA. It's a thin, typed wrapper
around the daemon's REST API, one service class per API area, plus a
shared HTTP client that handles auth cookies and typed errors.

## Install

```bash
npm install @wardnet/js
# or
yarn add @wardnet/js
```

Works in both browsers and Node 18+, since it's built on the native
`fetch` API.

## Quick start

```ts
import { WardnetClient, AuthService, DeviceService } from "@wardnet/js";

const client = new WardnetClient({ baseUrl: "http://192.168.1.1/api" });

const auth = new AuthService(client);
await auth.login({ username: "admin", password: "…" });
// The daemon sets a session cookie on the response; the client sends
// credentials on every subsequent request automatically.

const devices = new DeviceService(client);
const { devices: list } = await devices.list();
```

Every service class takes the same shared `client` instance in its
constructor, construct one `WardnetClient` per daemon you're talking
to and pass it to whichever services you need.

## Error handling

Failed requests throw `WardnetApiError`, which carries the HTTP
status, the daemon's structured error body, and the `X-Request-Id`
header when present, useful for correlating a client-side failure with
the daemon's own logs.

```ts
import { WardnetApiError } from "@wardnet/js";

try {
  await devices.getById("does-not-exist");
} catch (err) {
  if (err instanceof WardnetApiError) {
    console.error(err.status, err.body.error, err.requestId);
  }
}
```

## Services

Each service wraps one area of the daemon API. Follow a link for its
full method-by-method reference, generated directly from the SDK's
source and kept in sync on every release.

| Service | Covers |
| --- | --- |
| [AuthService](/sdk-docs/classes/AuthService.html) | Login, session refresh, current admin identity. |
| [DeviceService](/sdk-docs/classes/DeviceService.html) | Device listing, routing rules, DNS capture settings. |
| [TunnelService](/sdk-docs/classes/TunnelService.html) | WireGuard tunnel import, stats, latency, speed test. |
| [ProviderService](/sdk-docs/classes/ProviderService.html) | VPN provider catalog and per-provider config. |
| [NetworkService](/sdk-docs/classes/NetworkService.html) | LAN interface and gateway info. |
| [NetworkZonesService](/sdk-docs/classes/NetworkZonesService.html) | Network zone CRUD and enforcement. |
| [ZoneExceptionsService](/sdk-docs/classes/ZoneExceptionsService.html) | Per-device zone exceptions. |
| [DhcpService](/sdk-docs/classes/DhcpService.html) | DHCP reservations and lease info. |
| [DnsService](/sdk-docs/classes/DnsService.html) | DNS stats and query log. |
| [DnsFilterService](/sdk-docs/classes/DnsFilterService.html) | Blocklists, allowlists, filter profiles. |
| [DnsLocalService](/sdk-docs/classes/DnsLocalService.html) | Local DNS records and conditional forwarding. |
| [DnsLogStreamService](/sdk-docs/classes/DnsLogStreamService.html) | Live DNS query log streaming. |
| [RemoteAccessService](/sdk-docs/classes/RemoteAccessService.html) | Public hostname enrollment, certificate status. |
| [BackupService](/sdk-docs/classes/BackupService.html) | Encrypted backup export/import. |
| [UpdateService](/sdk-docs/classes/UpdateService.html) | Auto-update channel, status, manual install. |
| [SetupService](/sdk-docs/classes/SetupService.html) | First-run setup wizard steps. |
| [SystemService](/sdk-docs/classes/SystemService.html) | System info, logs, error ring buffer. |
| [StatsService](/sdk-docs/classes/StatsService.html) | Time-series and top-N metrics. |
| [InfoService](/sdk-docs/classes/InfoService.html) | Unauthenticated daemon version/health info. |
| [JobsService](/sdk-docs/classes/JobsService.html) | Background job status polling. |
| [AccessRequestService](/sdk-docs/classes/AccessRequestService.html) | Device-initiated access requests: domain allow/block, and Private DNS. |
| [PushService](/sdk-docs/classes/PushService.html) | Web Push subscription and notifications. |
| [LogService](/sdk-docs/classes/LogService.html) | Structured log streaming and filters. |

See the [full generated reference](/sdk-docs/index.html) for every
exported type, interface, and function, including all request/response
shapes.
