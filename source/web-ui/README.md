# Wardnet Web UI

React + TypeScript frontend for the Wardnet daemon.

## Local Development Against a Pi

The web UI dev server proxies API requests to the daemon. To develop locally while running the daemon on your Raspberry Pi:

### 1. Start the daemon on the Pi

```sh
make run-pi PI_HOST=10.232.1.195 PI_USER=pgomes PI_LAN_IF=eth1 OTEL=false RESUME=true
```

### 2. Open an SSH tunnel to forward the daemon port

```sh
ssh -N -L 7411:127.0.0.1:7411 pgomes@10.232.1.195
```

This makes the daemon API available at `http://localhost:7411` on your machine.

### 3. Start the dev server

```sh
cd source/web-ui
yarn dev
```

The Vite dev server proxies `/api` requests to `http://localhost:7411`.

### Self-service mode

Testing the self-service device page (`/my-device`) requires the daemon to see your real LAN IP so it can match you to a discovered device. When accessing through an SSH tunnel, the daemon sees `127.0.0.1` instead of your device's IP, so it cannot identify your device.

To test self-service mode, open the Pi's web UI directly in your browser (e.g. `http://10.232.1.195:7411`).
