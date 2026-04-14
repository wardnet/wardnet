import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { sleep } from "../runner.js";
import { services, agent } from "../helpers/setup.js";
import { env } from "../helpers/env.js";
import { state } from "../helpers/state.js";

export const steps: Step[] = [
  [
    "04a: assign test_alpine to tunnel 1",
    async () => {
      const res = await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "tunnel", tunnel_id: state.tunnel1Id },
      });
      assert.deepEqual(res.current_rule, {
        type: "tunnel",
        tunnel_id: state.tunnel1Id,
      });
      await sleep(2000);
    },
  ],
  [
    "04b: WireGuard interface wg_ward0 is up",
    async () => {
      const link = await agent.linkShow("wg_ward0");
      assert.ok(link.exists, "wg_ward0 should exist");
      assert.ok(link.up, "wg_ward0 should be UP");
    },
  ],
  [
    "04c: WireGuard peer is configured on wg_ward0",
    async () => {
      const wg = await agent.wgShow("wg_ward0");
      assert.ok(wg.exists, "wg_ward0 should exist");
      assert.ok(wg.peers && wg.peers.length >= 1, "expected at least 1 peer");
    },
  ],
  [
    "04d: ip rule exists for test_alpine",
    async () => {
      const rules = await agent.ipRules();
      const match = rules.rules.find(
        (r) => r.from === `${env.testAlpineIp}/32` || r.from === env.testAlpineIp,
      );
      assert.ok(match, `no ip rule for ${env.testAlpineIp}`);
      assert.equal(match.table, "100");
    },
  ],
  [
    "04e: nftables masquerade exists for wg_ward0",
    async () => {
      const nft = await agent.nftRules();
      assert.ok(
        nft.has_masquerade_for.includes("wg_ward0"),
        `expected masquerade for wg_ward0, got: ${JSON.stringify(nft.has_masquerade_for)}`,
      );
    },
  ],
  [
    "04f: remove routing rule (set to direct)",
    async () => {
      const res = await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "direct" },
      });
      assert.deepEqual(res.current_rule, { type: "direct" });
      await sleep(2000);
    },
  ],
  [
    "04g: ip rule removed for test_alpine",
    async () => {
      const rules = await agent.ipRules();
      const match = rules.rules.find(
        (r) => r.from === `${env.testAlpineIp}/32` || r.from === env.testAlpineIp,
      );
      assert.equal(match, undefined, "ip rule should be removed");
    },
  ],
];
