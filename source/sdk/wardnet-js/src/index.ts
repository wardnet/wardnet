// Logging
export { createLogger, setLevel } from "./logger.js";
export type { Logger, LogLevel } from "./logger.js";

// Client
export { WardnetClient, WardnetApiError } from "./client.js";
export type { WardnetClientOptions } from "./client.js";

// Services
export { AuthService } from "./services/auth.js";
export { DeviceService } from "./services/devices.js";
export { TunnelService } from "./services/tunnels.js";
export { ProviderService } from "./services/providers.js";
export { SystemService } from "./services/system.js";
export { NetworkService } from "./services/network.js";
export { NetworkZonesService } from "./services/network-zones.js";
export { ZoneExceptionsService } from "./services/zone-exceptions.js";
export { SetupService } from "./services/setup.js";
export { InfoService } from "./services/info.js";
export { DhcpService } from "./services/dhcp.js";
export { JobsService } from "./services/jobs.js";
export { RuleRequestService } from "./services/rule-requests.js";
export { InboundWgService } from "./services/inbound-wg.js";
export { PushService } from "./services/push.js";
export { LogService } from "./services/logs.js";
export type { LogEntry, LogFilter, LogStreamCallbacks } from "./services/logs.js";

// Types — push
export type {
  WebPushKeys,
  WebPushSubscription,
  VapidPublicKeyResponse,
  NotificationKind,
  NotificationItem,
  NotificationsResponse,
} from "./types/push.js";

// Types — jobs
export type { Job, JobKind, JobStatus, JobDispatchedResponse } from "./types/jobs.js";
export { isTerminal as isJobTerminal } from "./types/jobs.js";

// Types — devices
export type {
  Device,
  DeviceType,
  DhcpStatus,
  DeviceConnectionMode,
  RoutingTarget,
  RuleCreator,
  RoutingRule,
} from "./types/device.js";

// Types — network zones
export type {
  NetworkZone,
  NetworkZoneView,
  ZoneStance,
  ZoneProvenance,
  AllowedTargetKind,
  ZoneSubnet,
  ZoneSummary,
} from "./types/network-zone.js";

// Types — cross-zone exceptions
export type {
  ZoneException,
  ExceptionEndpoint,
  ExceptionEndpointKind,
  ServiceSpec,
  ServiceSet,
  PortSpec,
  Proto,
} from "./types/zone-exception.js";

// Types — tunnels
export type { Tunnel, TunnelStatus } from "./types/tunnel.js";

// Types — providers
export type {
  ProviderAuthMethod,
  ProviderInfo,
  ProviderCredentials,
  CountryInfo,
  ServerFilter,
  ServerInfo,
} from "./types/provider.js";

// Types — auth
export type { LoginRequest, LoginResponse, MeResponse } from "./types/auth.js";

// Types — system
export type {
  LastShutdownState,
  LastShutdownStatus,
  SetDefaultPolicyRequest,
  SetDefaultPolicyResponse,
  SystemStatusResponse,
} from "./types/system.js";

// Types — network
export type {
  DhcpSelfProbeResponse,
  DhcpSource,
  DiscoverGatewayMacRequest,
  DiscoverGatewayMacResponse,
  NetworkStatusResponse,
  RouterMacSource,
} from "./types/network.js";

// Types — setup
export type {
  AdvanceWizardRequest,
  AdvanceWizardResponse,
  SetupRequest,
  SetupResponse,
  SetupStatusResponse,
  WizardMode,
  WizardStep,
} from "./types/setup.js";

// Types — info
export type { InfoResponse } from "./types/info.js";

// Types — DHCP
export type {
  DhcpConfig,
  DhcpLease,
  DhcpLeaseStatus,
  DhcpReservation,
  DhcpConfigResponse,
  UpdateDhcpConfigRequest,
  PreviewDhcpConfigRequest,
  PreviewDhcpConfigResponse,
  ToggleDhcpRequest,
  ListDhcpLeasesResponse,
  ListDhcpReservationsResponse,
  CreateDhcpReservationRequest,
  CreateDhcpReservationResponse,
  DeleteDhcpReservationResponse,
  DhcpStatusResponse,
  RevokeDhcpLeaseResponse,
} from "./types/dhcp.js";

// Types — API DTOs
export type {
  ApiError,
  DeviceCaptureToggleRequest,
  RuleRequestKind,
  RuleRequestStatus,
  DeviceRuleRequest,
  CreateRuleRequestRequest,
  DecideRuleRequestRequest,
  DeviceMeResponse,
  SetMyRuleRequest,
  SetMyRuleResponse,
  ListDevicesResponse,
  DeviceDetailResponse,
  UpdateDeviceRequest,
  CreateTunnelRequest,
  CreateTunnelResponse,
  ListTunnelsResponse,
  DeleteTunnelResponse,
  RebuildTunnelResponse,
  TunnelDetailResponse,
  TunnelDevicesResponse,
  TunnelTestResult,
  TunnelTestResponse,
  TunnelSpeedTestResult,
  TunnelSpeedTestHistoryResponse,
  ListProvidersResponse,
  ValidateCredentialsRequest,
  ValidateCredentialsResponse,
  ListServersRequest,
  ListServersResponse,
  ListCountriesResponse,
  SetupProviderRequest,
  SetupProviderResponse,
  TunnelSummary,
  DnsCaptureSettingsRequest,
  DnsCaptureSettingsResponse,
  DnsEventItem,
  DnsEventsAckRequest,
  ListNetworkZonesResponse,
  GetNetworkZoneResponse,
  CreateNetworkZoneRequest,
  CreateNetworkZoneResponse,
  UpdateNetworkZoneRequest,
  UpdateNetworkZoneResponse,
  DeleteNetworkZoneResponse,
  AssignDeviceZoneRequest,
  QuarantineNewDevicesResponse,
  SetQuarantineNewDevicesRequest,
  ListZoneExceptionsResponse,
  GetZoneExceptionResponse,
  CreateZoneExceptionRequest,
  CreateZoneExceptionResponse,
  UpdateZoneExceptionRequest,
  UpdateZoneExceptionResponse,
  DeleteZoneExceptionResponse,
} from "./types/api.js";

// Services — DNS
export { DnsService } from "./services/dns.js";
export { DnsFilterService } from "./services/dns-filter.js";
export { DnsLocalService } from "./services/dns-local.js";
export { RoutingProfilesService } from "./services/routing-profiles.js";
export { DnsLogStreamService } from "./services/dnsLogStream.js";
export type { DnsLogStreamFilter, DnsLogStreamCallbacks } from "./services/dnsLogStream.js";

// Services — stats
export { StatsService } from "./services/stats.js";

// Services — auto-update
export { UpdateService } from "./services/update.js";

// Services — backup
export { BackupService } from "./services/backup.js";

// Services — remote access (DDNS + TLS)
export { RemoteAccessService } from "./services/remote-access.js";

// Types — remote access (DDNS + TLS)
export type {
  DdnsEnrollmentCodeRequest,
  DdnsEnrollRequest,
  DdnsCheckResponse,
  DdnsRegisterRequest,
  ConfigureCloudflareRequest,
  DdnsRegisterResponse,
  DdnsStatusResponse,
  DdnsResolutionVerdict,
  DdnsResolutionCheckResponse,
  TlsProvisioningPhase,
  TlsStatusResponse,
} from "./types/remote-access.js";

// Types — inbound WireGuard remote-access grants
export type {
  InboundWgConfigRequest,
  InboundWgConfigResponse,
  AddInboundWgPeerRequest,
  AddInboundWgPeerResponse,
  InboundWgPeerSummary,
  ListInboundWgPeersResponse,
  SetInboundWgPeerEnabledRequest,
} from "./types/inbound-wg.js";

// Types — backup
export type {
  BundleManifest,
  RestorePhase,
  BackupStatus,
  SnapshotKind,
  LocalSnapshot,
  BackupStatusResponse,
  ExportBackupRequest,
  RestorePreviewResponse,
  ApplyImportRequest,
  ApplyImportResponse,
  ListSnapshotsResponse,
} from "./types/backup.js";

// Types — auto-update
export type {
  UpdateChannel,
  UpdateHistoryStatus,
  InstallPhase,
  Release,
  UpdateHistoryEntry,
  InstallHandle,
  UpdateStatus,
  UpdateStatusResponse,
  UpdateCheckResponse,
  InstallUpdateRequest,
  InstallUpdateResponse,
  RollbackResponse,
  UpdateConfigRequest,
  UpdateConfigResponse,
  UpdateHistoryResponse,
} from "./types/update.js";

// Types — DNS (server config + query log)
export type {
  DnsProtocol,
  DnsResolutionMode,
  ForwarderSelectionMode,
  UpstreamDns,
  UpstreamLatency,
  DnsConfig,
  DnsConfigResponse,
  UpdateDnsConfigRequest,
  ToggleDnsRequest,
  DnsStatusResponse,
  DnsCacheFlushResponse,
  DnsQueryResult,
  DnsQueryLogEntry,
  QueryLogEvent,
  ListQueryLogParams,
  ListQueryLogResponse,
} from "./types/dns.js";

// Types — stats
export type {
  StatsBucket,
  StatsQuery,
  StatsSeriesPoint,
  StatsQueryResponse,
  StatsTopQuery,
  StatsTopEntry,
  StatsTopResponse,
} from "./types/stats.js";

// Types — DNS Filter (profiles, blocklists, allowlist, rules, per-device settings)
export type {
  DnsFilterProfile,
  DeviceDnsFilterSettings,
  DnsFilterConfig,
  Blocklist,
  AllowlistEntry,
  CustomFilterRule,
  ListProfilesResponse,
  GetProfileResponse,
  CreateProfileRequest,
  CreateProfileResponse,
  UpdateProfileRequest,
  UpdateProfileResponse,
  DeleteProfileResponse,
  ListBlocklistsResponse,
  CreateBlocklistRequest,
  CreateBlocklistResponse,
  UpdateBlocklistRequest,
  UpdateBlocklistResponse,
  DeleteBlocklistResponse,
  ListAllowlistResponse,
  CreateAllowlistRequest,
  CreateAllowlistResponse,
  DeleteAllowlistResponse,
  ListFilterRulesResponse,
  CreateFilterRuleRequest,
  CreateFilterRuleResponse,
  UpdateFilterRuleRequest,
  UpdateFilterRuleResponse,
  DeleteFilterRuleResponse,
  ListDeviceFilterSettingsParams,
  ListDeviceFilterSettingsResponse,
  GetDeviceFilterSettingsResponse,
  UpdateDeviceFilterSettingsRequest,
  UpdateDeviceFilterSettingsResponse,
  DnsFilterConfigResponse,
  UpdateDnsFilterConfigRequest,
} from "./types/dns-filter.js";

// Types — Local DNS (authoritative zones, custom records, forwarding rules)
export type {
  DnsRecordType,
  DnsRecordSource,
  DnsZoneSource,
  DnsZone,
  CustomDnsRecord,
  ConditionalForwardingRule,
  ListZonesResponse,
  GetZoneResponse,
  CreateZoneRequest,
  CreateZoneResponse,
  UpdateZoneRequest,
  UpdateZoneResponse,
  DeleteZoneResponse,
  ListRecordsResponse,
  GetRecordResponse,
  CreateRecordRequest,
  CreateRecordResponse,
  UpdateRecordRequest,
  UpdateRecordResponse,
  DeleteRecordResponse,
  ListForwardingRulesResponse,
  GetForwardingRuleResponse,
  CreateForwardingRuleRequest,
  CreateForwardingRuleResponse,
  UpdateForwardingRuleRequest,
  UpdateForwardingRuleResponse,
  DeleteForwardingRuleResponse,
} from "./types/dns-local.js";

// Types — Routing profiles (per-domain routing rules)
export type {
  DomainRoutingTarget,
  RoutingProfile,
  DomainRoutingRule,
  ListRoutingProfilesResponse,
  GetRoutingProfileResponse,
  CreateRoutingProfileRequest,
  CreateRoutingProfileResponse,
  UpdateRoutingProfileRequest,
  UpdateRoutingProfileResponse,
  DeleteRoutingProfileResponse,
  ListDomainRoutingRulesResponse,
  CreateDomainRoutingRuleRequest,
  CreateDomainRoutingRuleResponse,
  UpdateDomainRoutingRuleRequest,
  UpdateDomainRoutingRuleResponse,
  DeleteDomainRoutingRuleResponse,
  GetDeviceRoutingProfilesResponse,
  SetDeviceRoutingProfilesRequest,
  SetDeviceRoutingProfilesResponse,
} from "./types/routing-profiles.js";

// Utils
export { isCompleteIpv4, ipv4ToInt } from "./utils/ipv4.js";
