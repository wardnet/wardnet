import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { services, agent } from "../helpers/setup.js";
import { state } from "../helpers/state.js";

export const steps: Step[] = [
  [
    "02a: import tunnel 1 from WireGuard config",
    async () => {
      const config = await agent.readFixture("tunnel-1.conf");
      assert.ok(config.includes("[Interface]"), "expected WireGuard config");

      const res = await services.tunnels.create({
        label: "Test Tunnel 1",
        country_code: "XX",
        config,
      });
      assert.equal(res.tunnel.label, "Test Tunnel 1");
      assert.equal(res.tunnel.status, "down");
      state.tunnel1Id = res.tunnel.id;
    },
  ],
  [
    "02b: import tunnel 2 from WireGuard config",
    async () => {
      const config = await agent.readFixture("tunnel-2.conf");

      const res = await services.tunnels.create({
        label: "Test Tunnel 2",
        country_code: "YY",
        config,
      });
      assert.equal(res.tunnel.label, "Test Tunnel 2");
      state.tunnel2Id = res.tunnel.id;
    },
  ],
  [
    "02c: GET /api/tunnels lists both tunnels",
    async () => {
      const res = await services.tunnels.list();
      assert.equal(res.tunnels.length, 2);
      const labels = res.tunnels.map((t) => t.label);
      assert.ok(labels.includes("Test Tunnel 1"));
      assert.ok(labels.includes("Test Tunnel 2"));
    },
  ],
];
