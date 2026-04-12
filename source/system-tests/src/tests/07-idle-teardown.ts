import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { sleep } from "../runner.js";
import { agent } from "../helpers/setup.js";
import { env } from "../helpers/env.js";

export const steps: Step[] = [
  [
    "07a: wait for idle timeout",
    async () => {
      const waitMs = (env.idleTimeoutSecs + 3) * 1000;
      await sleep(waitMs);
    },
  ],
  [
    "07b: wg_ward0 removed after idle",
    async () => {
      const link = await agent.linkShow("wg_ward0");
      assert.ok(!link.exists, "wg_ward0 should be removed after idle");
    },
  ],
  [
    "07c: wg_ward1 removed after idle",
    async () => {
      const link = await agent.linkShow("wg_ward1");
      assert.ok(!link.exists, "wg_ward1 should be removed after idle");
    },
  ],
  [
    "07d: no leftover ip rules for test devices",
    async () => {
      const rules = await agent.ipRules();
      const leftover = rules.rules.filter(
        (r) => r.from.startsWith(env.testAlpineIp) || r.from.startsWith(env.testUbuntuIp),
      );
      assert.equal(leftover.length, 0, `leftover rules: ${JSON.stringify(leftover)}`);
    },
  ],
];
