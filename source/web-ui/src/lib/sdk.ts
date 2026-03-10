import {
  WardnetClient,
  AuthService,
  DeviceService,
  TunnelService,
  ProviderService,
  SystemService,
  SetupService,
  InfoService,
} from "@wardnet/js";

/** Shared SDK client instance. All services use this single client. */
export const client = new WardnetClient();

export const authService = new AuthService(client);
export const deviceService = new DeviceService(client);
export const tunnelService = new TunnelService(client);
export const providerService = new ProviderService(client);
export const systemService = new SystemService(client);
export const setupService = new SetupService(client);
export const infoService = new InfoService(client);
