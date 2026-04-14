import {
  AuthService,
  SetupService,
  TunnelService,
  DeviceService,
  InfoService,
  SystemService,
} from "@wardnet/js";

import { TestApiClient } from "./client.js";
import { TestAgent } from "./agent.js";
import { env } from "./env.js";

/** Shared API client instance (with cookie session management). */
export const api = new TestApiClient(env.apiUrl);

/** Shared test agent instance (kernel/container operations on the Pi). */
export const agent = new TestAgent(env.agentUrl);

/** SDK services wired to the test API client. */
export const services = {
  auth: new AuthService(api),
  setup: new SetupService(api),
  tunnels: new TunnelService(api),
  devices: new DeviceService(api),
  info: new InfoService(api),
  system: new SystemService(api),
};
