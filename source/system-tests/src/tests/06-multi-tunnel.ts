import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { sleep } from "../runner.js";
import { services, agent } from "../helpers/setup.js";
import { env } from "../helpers/env.js";
import { state } from "../helpers/state.js";

export const steps: Step[] = [
  [
    "06a: assign alpine → tunnel 1, ubuntu → tunnel 2",
    async () => {
      await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "tunnel", tunnel_id: state.tunnel1Id },
      });
      await services.devices.update(state.ubuntuDeviceId, {
        routing_target: { type: "tunnel", tunnel_id: state.tunnel2Id },
      });
      await sleep(5000);
    },
  ],
  [
    "06b: test_alpine reaches peer 1 but NOT peer 2",
    async () => {
      const r1 = await agent.ping("wardnet_test_alpine", env.mockPeer1Internal, 3, 3);
      assert.ok(r1, "alpine should reach peer 1");
      const r2 = await agent.ping("wardnet_test_alpine", env.mockPeer2Internal, 1, 2);
      assert.ok(!r2, "alpine should NOT reach peer 2");
    },
  ],
  [
    "06c: test_ubuntu reaches peer 2 but NOT peer 1",
    async () => {
      const execResult = await agent.containerExec("wardnet_test_ubuntu", [
        "ping", "-c", "3", "-W", "3", env.mockPeer2Internal,
      ]);
      const r2 = execResult.exit_code === 0;
      assert.ok(r2, `ubuntu should reach peer 2 (exit=${execResult.exit_code}, stdout=${execResult.stdout.slice(0, 200)}, stderr=${execResult.stderr.slice(0, 200)})`);
      const r1 = await agent.ping("wardnet_test_ubuntu", env.mockPeer1Internal, 1, 2);
      assert.ok(!r1, "ubuntu should NOT reach peer 1");
    },
  ],
  [
    "06d: clean up — remove both rules",
    async () => {
      await services.devices.update(state.alpineDeviceId, {
        routing_target: { type: "direct" },
      });
      await services.devices.update(state.ubuntuDeviceId, {
        routing_target: { type: "direct" },
      });
      await sleep(1000);
    },
  ],
];
