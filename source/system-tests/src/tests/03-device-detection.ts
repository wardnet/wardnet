import assert from "node:assert/strict";
import type { Step } from "../runner.js";
import { services } from "../helpers/setup.js";
import { env } from "../helpers/env.js";
import { state } from "../helpers/state.js";

export const steps: Step[] = [
  [
    "03a: test_alpine detected via ARP scan",
    async () => {
      const res = await services.devices.list();
      assert.ok(res.devices.length >= 1, `expected devices, got ${res.devices.length}`);
      const alpine = res.devices.find((d) => d.last_ip === env.testAlpineIp);
      assert.ok(alpine, `no device with IP ${env.testAlpineIp}`);
      state.alpineDeviceId = alpine.id;
    },
  ],
  [
    "03b: test_ubuntu detected via ARP scan",
    async () => {
      const res = await services.devices.list();
      const ubuntu = res.devices.find((d) => d.last_ip === env.testUbuntuIp);
      assert.ok(ubuntu, `no device with IP ${env.testUbuntuIp}`);
      state.ubuntuDeviceId = ubuntu.id;
    },
  ],
];
