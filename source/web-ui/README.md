# Wardnet Web UI

React + TypeScript frontend for the Wardnet daemon.

## Local development

The fastest dev loop runs the mock daemon and the Vite dev server together:

```sh
make run-dev
```

- Mock API on `http://localhost:7411` (full HTTP surface, no-op network backends, seeded demo data).
- Vite dev server on `http://localhost:7412`, proxying `/api` to the mock.
- `RESUME=true` persists the mock's SQLite at `.wardnet-local/wardnet.db` between runs.

Run the pieces independently with `make run-dev-daemon` (mock only on `:7411`) and `make run-dev-web` (Vite only on `:7412`).

### Against the real daemon

For features that need real backends (WireGuard, DHCP, DNS, device discovery, `/my-device`), run the daemon in a container via `make image` and point `yarn dev` at it — the Vite proxy already targets `http://localhost:7411`.

The `/my-device` page only matches a device when the daemon sees the request's source IP on its LAN, so reach the UI via the daemon's published port (e.g. `http://localhost:7411`) instead of the Vite dev server when testing self-service flows.
