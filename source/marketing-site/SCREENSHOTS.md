# Capturing docs screenshots

How the images under `public/docs/<slug>/` are produced. Follow this so
every shot is the same size, needs no crop, and looks like one coherent
set. This replaces the old macOS `⌘⇧4` + ImageMagick-crop workflow.

## Method: chrome-devtools MCP (CDP)

The Chrome DevTools MCP takes a chromeless screenshot of the page at an
emulated viewport, so there is **no browser chrome to crop** and every
image is deterministic.

1. **Start the stack:** `make run-dev` (mock daemon on `:7411`,
   admin-site on `:7412/admin/`, user PWA `:7413`, admin PWA `:7414`).
   The mock seeds demo data from
   `source/daemon/crates/wardnetd-mock/src/seed.rs`.
2. **Fresh seed when needed:** the mock reuses `.wardnet-local/wardnet.db`
   if present. To re-seed after a `seed.rs` change:
   `rm -f .wardnet-local/wardnet.db*` then restart `make run-dev`. A fresh
   DB re-runs the setup wizard (see below).
3. **Open a page:** `new_page http://127.0.0.1:7412/admin/`
4. **Set the viewport once:** `emulate` with `viewport: "1600x1100x2"`
   (width × height × devicePixelRatio). This is the single standard size —
   do not change it between shots.
5. **Capture:** `take_screenshot` with an absolute `filePath` **inside the
   repo** (the MCP sandboxes writes to the workspace root). At
   `1600x1100x2` this yields a **3200×2200** PNG of just the app — no tabs,
   no URL bar, no window margin, no crop.

### Before every shot

- **Dismiss the unclean-shutdown banner:** click the button whose
  `aria-label` starts with `Dismiss` (the "Wardnet did not shut down
  cleanly" alert). One-liner:
  `[...document.querySelectorAll('button')].find(b=>/dismiss/i.test(b.getAttribute('aria-label')||''))?.click()`
- **Scroll the right thing:** the main panel scrolls **internally**
  (element with class `scroll`), not the window. To frame a section, find
  its heading and set the scroll container's `scrollTop` so the heading
  sits ~150px from the top:
  ```js
  const h = [...document.querySelectorAll('h2,h3')].find(x => x.innerText.startsWith('SECTION'));
  let sc = h; while (sc && !(sc.scrollHeight > sc.clientHeight + 5)) sc = sc.parentElement;
  sc.scrollTop += h.getBoundingClientRect().top - 150;
  ```
  Some sections sit at the page bottom; the container maxes out and the
  heading can't reach the top — that's fine, capture what fits.
- **Driving controls without huge a11y snapshots:** `take_snapshot`
  output is large (inline logo SVG). Prefer `evaluate_script` to click by
  text and to fill React inputs via the native value setter:
  ```js
  const setVal = (el, v) => { Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype, 'value').set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true })); };
  ```

### The setup wizard (once per fresh DB)

The mock never seeds an admin account, so a fresh DB always lands on
`/admin/setup`. Walk it once, then the seeded data is reachable. It is
also the source of the `first-run/*` shots — capture each step here.

| Step | Action |
| --- | --- |
| 1 Admin | Fill username `admin` + both password fields, click **Create account** |
| 2 Network | **Continue** |
| 3 DHCP | Click **Probe LAN**, wait for "Only Wardnet responded", **Continue** |
| 4 Router | **Skip** (ARP probe) |
| 5 DNS | **Continue** |
| 6 Tunnel | **Continue** ("3 tunnels configured" from seed) |
| 7 Policy | **Continue** |
| 8 HTTPS | **Skip for now** |
| 9 Review | **Finish setup** |
| 10 Done | **Go to dashboard** |

## Place and reference

- Save to `public/docs/<slug>/<name>.png`.
- Reference from `content/docs/<slug>.md` (and reused by
  `content/blog/*.md`) with `![alt](/docs/<slug>/<name>.png "wide")`. The
  `"wide"` title renders full-width in `DocsArticle.tsx`.

## Recipe catalog

Every shipped screenshot, with the exact steps to reproduce it. "Nav"
means `new_page`/`navigate` to that path; assume the dismiss-banner +
scroll-to-top preamble before each capture unless noted.

### first-run/ (11) — captured during the wizard walk above

| File | Wizard step |
| --- | --- |
| `01-admin.png` | Step 1, form filled, before Create |
| `02-network.png` | Step 2 |
| `03-dhcp.png` | Step 3, before probe |
| `03b-dhcp-probe.png` | Step 3, after a clean probe |
| `04-router.png` | Step 4 |
| `05-dns.png` | Step 5 |
| `06-tunnel.png` | Step 6 |
| `07-policy.png` | Step 7 |
| `08-https.png` | Step 8 |
| `09-review.png` | Step 9 |
| `10-done.png` | Step 10 ("All set") |

### Admin pages (navigate + capture)

| File | Route | Setup / framing |
| --- | --- | --- |
| `app-surfaces/desktop-admin.png` | `/admin` | Dashboard, top of page |
| `device-routing/devices-list.png` | `/admin/devices` | "All" tab (10 devices) |
| `dhcp-server/dhcp-page.png` | `/admin/dhcp` | Top of page |
| `dhcp-server/leases-table.png` | `/admin/dhcp` | Click the **Leases** tab |
| `dns-ad-blocking/filtering-page.png` | `/admin/dns/filter` | Top of page |
| `dns-ad-blocking/profile-detail.png` | `/admin/dns/filter` | Click the **Ad Blocking** profile row (→ `/profiles/00000000-…-000000000100`) |
| `dns-ad-blocking/query-log.png` | `/admin/dns/logs` | Top of page |
| `local-dns/local-dns-page.png` | `/admin/dns/local` | Top of page |
| `network-zones/zones-page.png` | `/admin/zones` | Shows zone counts + casting exception |
| `wireguard-tunnels/tunnels-list.png` | `/admin/tunnels` | Top of page |

### Stateful admin shots

| File | Route | Setup / interaction |
| --- | --- | --- |
| `dns-ad-blocking/dns-stats.png` | `/admin/dns` | **Apply PR #940's `Math.round` to `DnsStatsSection` `windowTotals` first** (temporarily, until it merges) so the "Window total" subtitle is integer. Scroll so **QUERIES OVER TIME** is ~210px from top (shows stat cards + chart + Top blocked/Top clients). |
| `device-routing/routing-edit.png` | `/admin/devices` → open **living-room-tv** | On the device page, click **Edit** in the SETTINGS card, then click the **Routing** dropdown so all four targets show (Direct + 3 tunnels). |
| `network-zones/device-zone-edit.png` | `/admin/devices` → open **living-room-tv** | Cancel any Settings edit, click **Edit** in the ZONE card, open the zone dropdown (Guest / IoT / Trusted). |
| `wireguard-tunnels/detail-overview.png` | `/admin/tunnels` → open the **Home lab** tunnel (the "up" tunnel with 2 routed devices) | Top of the tunnel detail page. |
| `wireguard-tunnels/devices-table.png` | same **Home lab** tunnel | Scroll to **DEVICES USING THIS TUNNEL**. |
| `wireguard-tunnels/throughput-chart.png` | any tunnel detail | Scroll to **THROUGHPUT**. |
| `wireguard-tunnels/latency-chart.png` | any tunnel detail | Scroll to **LATENCY**. |

### backup-restore/ (4) — `/admin/backups`

The backup/restore UI is **inline** (expanding sections), not modal dialogs.

| File | Steps |
| --- | --- |
| `backups-page.png` | Nav `/admin/backups`, capture top. |
| `export-dialog.png` | Click **Download backup** (expands the BACKUP passphrase form), fill both passphrase fields (≥12 chars, e.g. `correct-horse-battery-staple`), capture. **Do not** click Download (that streams a file). |
| `restore-upload-dialog.png` | Click **Restore…** (reveals the RESTORE section: file picker + passphrase + Preview), scroll to **RESTORE**, capture with empty fields. |
| `restore-preview-dialog.png` | Needs a real bundle. Generate one in-browser and feed the real Preview — no disk round-trip:<br>`evaluate_script`: `fetch('/api/backup/export',{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify({passphrase:PASS})})` → `blob()` → `new File([blob],'x.wardnet.age')` → assign to `input[type=file]` via a `DataTransfer` and dispatch `change` → fill the RESTORE passphrase with the same `PASS` → click **Preview**. The manifest (version / host ID / schema / "Will replace…") renders in-place; scroll to RESTORE and capture. |

### vpn-providers/ (1)

| File | Steps |
| --- | --- |
| `provider-tab.png` | Nav `/admin/tunnels`, click **Add tunnel** (reveals the "ADD WIREGUARD TUNNEL" form), click the **Provider** tab (radix tab — dispatch `mousedown`+`mouseup`+`click` if a plain `.click()` doesn't switch), scroll the form to top, capture. |

### wireguard-tunnels/ speed test (2)

| File | Steps |
| --- | --- |
| `speed-test.png` | Nav `/admin/tunnels`. Each tunnel card has **Speed test** (expands an inline Direct-vs-Tunnel comparison) and **Test** (runs a new one). Click **Speed test** on a tunnel; the mock returns a result (Download/Latency/Jitter, "% kept"). Scroll the expanded panel into view and capture. |
| `speed-test-history.png` | The tunnel **detail** page has a **SPEED TEST HISTORY** section. Populate it by firing a few runs: `evaluate_script` loop `fetch('/api/tunnels/{id}/speed-test',{method:'POST',credentials:'include'})` (3×, ~900ms apart), then reload, scroll to **SPEED TEST HISTORY**, capture. Results are also readable at `GET /api/tunnels/{id}/speed-test/results`. |

### remote-access/ (5) — `/admin/remote-access`

The mock seeds remote access **live** (`home1.demo.wardnet.services`, valid cert) and **fully simulates** the wardnet.services enrollment (Send code → any code verifies → suggested slug). All radios are real `<input type=radio>` — click the input directly (not the label) to switch.

| File | Steps |
| --- | --- |
| `status.png` | Nav `/admin/remote-access`, capture (STATUS = "Remote access is live"). |
| `enroll-wardnet.png` | Click **Change provider**, keep **Wardnet** selected, scroll to CHANGE PROVIDER, capture (email + Send code). |
| `enroll-cloudflare.png` | In Change provider, click the **Cloudflare** radio `input` (index 1) → Domain + API-token fields appear; capture. |
| `enroll-code.png` | Select the **Wardnet** radio `input` (index 0), fill the email (`demo@example.com`), click **Send code** → "We emailed a one-time code…" + code input; capture. |
| `enroll-slug.png` | Fill the code (any value, e.g. `123456`), click **Verify code** → suggested hostname ("… is available" + Register); capture. **Do not** click Register (mutates the live config). Click **Cancel** when done. |

### personal-vpn/ desktop (2) — `/admin/vpn`

The inbound-WireGuard (personal VPN) grant flow has real preconditions: the
**server must be enabled with a valid listen port**, and only a **named**
(managed) device with no existing peer is grantable
(`managedDevices = devices.filter(d => d.name != null)`).

1. **Name a device.** The "To Managed Device" toggle alone is not enough —
   the device needs a display `name`. Quickest: `PUT /api/devices/{id}`
   with `{name:"Alice's Phone", device_type:"phone"}` (fetch, `credentials:'include'`).
2. Nav `/admin/vpn`. Set **Listen port** (e.g. `51820`) so the enable
   switch is enabled, then toggle **Enable inbound WireGuard server**. The
   PEERS card appears; the config persists in `system_config`, but the
   toggle can read back off if the port was invalid — always set the port
   first.
3. Click **Grant access** → **Choose a device** (select the named device) →
   **Grant**.

| File | Steps |
| --- | --- |
| `grant-qr.png` | Capture the **"Remote access granted"** modal that appears after Grant — real WireGuard QR + Download .conf / I've saved this. |
| `server-peers.png` | Close the modal, scroll to **PEERS**, capture the peers table (device / tunnel IP / status). |

### mobile-apps/ + personal-vpn mobile — PWA surfaces

Mobile shots use the **user PWA** (`:7413/`) and **admin PWA**
(`:7414/admin-app/`) at a **phone viewport** (`emulate viewport "402x874x3"`,
≈ the 1000×1864 legacy size). Capture only the ones where the app version
is visible (per the refresh scope). The admin PWA shares the daemon
session; the user PWA resolves `/devices/me` to the localhost device.

Both PWAs show the version in the header, so all of these qualify. Dismiss
the "Install / Add to home screen" banner (**Later**) first.

**user PWA** (`:7413/app/`) — no login (device-keyed to `alice-laptop`):

| File | Steps |
| --- | --- |
| `user-home.png` | `/app/` — route + zone + route-verify map. |
| `user-stats.png` | Bottom nav **Stats**. |
| `user-settings.png` | Bottom nav **Settings**. |
| `user-ask-admin.png` | On **Stats**, tap an "Ask admin about `<domain>`" action on a domain row → the "Ask your administrator" modal; capture. |

**admin PWA** (`:7414/admin-app/`) — shares the daemon session (no login in dev):

| File | Steps |
| --- | --- |
| `mobile-apps/admin-dashboard.png` | `/admin-app/` home. |
| `mobile-apps/admin-system.png` | Bottom nav **System**. |
| `mobile-apps/admin-device-routing-sheet.png` | **Devices** tab → tap a device row (`<button>`) → the routing Drawer (route + zone). |
| `personal-vpn/admin-grant-sheet.png` | Same routing Drawer, but for a **named** device (`PUT /api/devices/{id}` `{name,device_type}` first) with the WG server enabled → a **REMOTE ACCESS** section with **Grant remote access** shows. Capture the sheet. |
| `personal-vpn/admin-grant-qr.png` | Tap **Grant remote access**; the Drawer swaps to the QR view ("REMOTE ACCESS GRANTED"). Capture. |

> Note: the grant needs the inbound WG server enabled once (see personal-vpn desktop recipe). A device can only be granted once — use a different named device for a fresh sheet/QR pair.

