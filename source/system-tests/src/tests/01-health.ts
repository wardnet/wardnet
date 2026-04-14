import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { services, agent } from "../helpers/setup.js";
import { env } from "../helpers/env.js";

export const steps: Step[] = [
  [
    "01a: test agent is reachable",
    async () => {
      const res = await agent.health();
      assert.equal(res.status, "ok");
    },
  ],
  [
    "01b: GET /api/info returns server info",
    async () => {
      const info = await services.info.getInfo();
      assert.ok(info.version, "expected version in info response");
    },
  ],
  [
    "01c: setup wizard creates admin account",
    async () => {
      const res = await services.setup.setup({
        username: env.adminUser,
        password: env.adminPass,
      });
      assert.ok(res.message);
    },
  ],
  [
    "01d: admin login succeeds",
    async () => {
      const res = await services.auth.login({
        username: env.adminUser,
        password: env.adminPass,
      });
      assert.ok(res.message);
    },
  ],
  [
    "01e: authenticated GET /api/tunnels returns empty list",
    async () => {
      const res = await services.tunnels.list();
      assert.deepEqual(res.tunnels, []);
    },
  ],
];
