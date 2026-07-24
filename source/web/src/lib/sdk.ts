import {
  WardnetClient,
  AuthService,
  BackupService,
  DeviceService,
  TunnelService,
  ProviderService,
  SystemService,
  NetworkService,
  SetupService,
  InfoService,
  DhcpService,
  DnsService,
  DnsFilterService,
  DnsLocalService,
  RoutingProfilesService,
  DnsLogStreamService,
  JobsService,
  LogService,
  StatsService,
  UpdateService,
  RemoteAccessService,
  RuleRequestService,
  NetworkZonesService,
  ZoneExceptionsService,
  PushService,
  InboundWgService,
  PrivateDnsService,
} from "@wardnet/js";

/** Shared SDK client instance. All services use this single client. */
export const client = new WardnetClient();

export const authService = new AuthService(client);
export const deviceService = new DeviceService(client);
export const tunnelService = new TunnelService(client);
export const providerService = new ProviderService(client);
export const systemService = new SystemService(client);
export const networkService = new NetworkService(client);
export const setupService = new SetupService(client);
export const infoService = new InfoService(client);
export const dhcpService = new DhcpService(client);
export const dnsService = new DnsService(client);
export const dnsFilterService = new DnsFilterService(client);
export const dnsLocalService = new DnsLocalService(client);
export const routingProfilesService = new RoutingProfilesService(client);
export const dnsLogStreamService = new DnsLogStreamService(
  client,
  window.location.origin,
);
export const jobsService = new JobsService(client);
export const logService = new LogService(client, window.location.origin);
export const statsService = new StatsService(client);
export const updateService = new UpdateService(client);
export const backupService = new BackupService(client);
export const remoteAccessService = new RemoteAccessService(client);
export const ruleRequestService = new RuleRequestService(client);
export const networkZonesService = new NetworkZonesService(client);
export const zoneExceptionsService = new ZoneExceptionsService(client);
export const pushService = new PushService(client);
export const inboundWgService = new InboundWgService(client);
export const privateDnsService = new PrivateDnsService(client);
