import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { sleep } from "../runner.js";
import { services, agent } from "../helpers/setup.js";
import { env } from "../helpers/env.js";
import { state } from "../helpers/state.js";

export const steps: Step[] = [
  [
    "05a: assign test_alpine to tunnel 1",
    async () => {
      await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "tunnel", tunnel_id: state.tunnel1Id },
      });
      await sleep(3000);
    },
  ],
  [
    "05b: traffic from test_alpine reaches mock peer 1",
    async () => {
      const reachable = await agent.ping("wardnet_test_alpine", env.mockPeer1Internal, 3, 3);
      assert.ok(reachable, `ping to ${env.mockPeer1Internal} failed`);
    },
  ],
  [
    "05c: traffic from test_alpine does NOT reach mock peer 2",
    async () => {
      const reachable = await agent.ping("wardnet_test_alpine", env.mockPeer2Internal, 1, 2);
      assert.ok(!reachable, `ping to ${env.mockPeer2Internal} should fail`);
    },
  ],
  [
    "05d: remove rule — ip rule is cleaned up",
    async () => {
      await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "direct" },
      });
      await sleep(2000);
      // The ip rule should be gone. Note: the tunnel's connected subnet
      // (10.99.1.0/24) remains reachable while wg_ward0 is UP — that's
      // correct behavior. ip rules only control default-route traffic.
      const rules = await agent.ipRules();
      const match = rules.rules.find(
        (r) => r.from === `${env.testAlpineIp}/32` || r.from === env.testAlpineIp,
      );
      assert.equal(match, undefined, "ip rule should be removed after setting direct");
    },
  ],
];
