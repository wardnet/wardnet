// Design system — re-exported from @wardnet/ui so existing consumers can keep
// importing primitives from @wardnet/web. New surfaces should import design-
// system components directly from @wardnet/ui.
export * from "@wardnet/ui";

// SDK singletons
export {
  client,
  authService,
  deviceService,
  tunnelService,
  providerService,
  systemService,
  networkService,
  setupService,
  infoService,
  dhcpService,
  dnsService,
  dnsFilterService,
  dnsLocalService,
  dnsLogStreamService,
  jobsService,
  logService,
  statsService,
  updateService,
  backupService,
} from "./lib/sdk";

// Format utilities
export {
  cn,
  formatBytes,
  formatMbps,
  formatMs,
  retentionPct,
  formatUptime,
  formatDate,
  formatTime,
  formatTimeShort,
  formatDateTime,
  timeAgo,
  apiErrorMessage,
  apiRequestId,
  deviceDisplayName,
  suggestHostnameForMac,
} from "./lib/utils";

// Country / device helpers
export { countryFlag } from "./lib/country";
export { tunnelStatusVariant, tunnelStatusLabel } from "./lib/tunnel";
export { TunnelStatusPill } from "./components/TunnelStatusPill";
export {
  DEVICE_TYPE_OPTIONS,
  deviceTypeLabel,
  isDeviceOnline,
} from "./lib/device";
export { cronToHuman } from "./lib/cron";
export { logger, createLogger, setLevel } from "./lib/logger";
export type { Logger, LogLevel } from "./lib/logger";

// Auth store
export { useAuthStore } from "./stores/authStore";

// Components
export { ApiErrorAlert } from "./components/ApiErrorAlert";
export { AppHeader } from "./components/AppHeader";
export { ConnectionGate } from "./components/ConnectionGate";
export type { ConnState } from "./components/AppHeader";
export { RuleRequestStatusPill } from "./components/RuleRequestStatusPill";
export { DeviceIcon } from "./components/DeviceIcon";
export { JobProgressDescription } from "./components/JobProgressDescription";
export { LoginForm } from "./components/LoginForm";
export { RoutingSelector } from "./components/RoutingSelector";
export { Ipv4Input } from "./components/Ipv4Input";
export { SubnetInput } from "./components/SubnetInput";

// CIDR helpers
export {
  parseCidr,
  buildCidr,
  isValidCidr,
  isPrivateCidr,
  isPrivateIpv4,
  octetsPrivate,
  usableHosts,
  prefixForHosts,
  networkOctets,
  octetsValid,
} from "./lib/cidr";
export type { ParsedCidr } from "./lib/cidr";

// Hooks — auth
export { useAuth, useMe } from "./hooks/useAuth";

// Hooks — push notifications (issues #482/#764)
export { usePushNotifications } from "./hooks/usePushNotifications";
export type { PushPermissionState } from "./hooks/usePushNotifications";
export {
  useRecentNotifications,
  useClearNotifications,
} from "./hooks/useRecentNotifications";
export type { PushPayload, PushNotificationData } from "./lib/pushPayload";
export { urlBase64ToUint8Array } from "./lib/pushPayload";
export { isIosBrowserTab } from "./lib/platform";

// Hooks — devices
export {
  useDevices,
  useDevice,
  useMyDevice,
  useSetMyRule,
  useSetMyCaptureEnabled,
  useUpdateDevice,
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
} from "./hooks/useDevices";

// Hooks — rule requests
export {
  useMyRuleRequests,
  useCreateRuleRequest,
  useRuleRequests,
  useDecideRuleRequest,
} from "./hooks/useRuleRequests";

// Hooks — tunnels
export {
  useTunnels,
  useTunnel,
  useTunnelDevices,
  useCreateTunnel,
  useDeleteTunnel,
  useTestTunnel,
  useRebuildTunnel,
  useSetTunnelDnsOverride,
  useSpeedTestResults,
  useStartSpeedTest,
} from "./hooks/useTunnels";

// Hooks — providers
export {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "./hooks/useProviders";

// Hooks — system
export {
  useSystemStatus,
  useAcknowledgeShutdown,
  useRecentErrors,
} from "./hooks/useSystemStatus";
export type { RecentError } from "./hooks/useSystemStatus";

export {
  useDefaultPolicy,
  useSetDefaultPolicy,
} from "./hooks/useDefaultPolicy";

// Hooks — daemon lifecycle
export { useDaemonStatus } from "./hooks/useDaemonStatus";
export { useDaemonReachability } from "./hooks/useDaemonReachability";
export type {
  DaemonReachabilityPhase,
  ReachabilityMode,
} from "./hooks/useDaemonReachability";
export { useRestart } from "./hooks/useRestart";
export type { RestartPhase } from "./hooks/useRestart";
export { useReboot } from "./hooks/useReboot";
export type { RebootPhase } from "./hooks/useReboot";
export { useShutdown } from "./hooks/useShutdown";
export type { ShutdownPhase } from "./hooks/useShutdown";

// Hooks — stats
export {
  useDnsStatSummary,
  useDnsStatsDashboard,
  useDashboardDnsStats,
  useDnsTopBlockedDomains,
  parseLabels,
  RANGES,
  RANGE_HOURS,
} from "./hooks/useStats";
export type {
  StatsRange,
  DnsStatsDashboardData,
  DashboardDnsStats,
} from "./hooks/useStats";
export { useTunnelStats } from "./hooks/useTunnelStats";
export type { TunnelStatsPoint, TunnelStatsData } from "./hooks/useTunnelStats";
export { useCombinedTunnelStats } from "./hooks/useCombinedTunnelStats";
export type { CombinedTunnelStatsData } from "./hooks/useCombinedTunnelStats";

// Hooks — DNS
export {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useUpdateDnsConfig,
  useFlushDnsCache,
} from "./hooks/useDns";

export {
  useDnsZones,
  useDnsZone,
  useDnsZoneRecords,
  useCreateDnsZone,
  useUpdateDnsZone,
  useDeleteDnsZone,
  useDnsRecords,
  useDnsRecord,
  useCreateDnsRecord,
  useUpdateDnsRecord,
  useDeleteDnsRecord,
  useForwardingRules,
  useForwardingRule,
  useCreateForwardingRule,
  useUpdateForwardingRule,
  useDeleteForwardingRule,
} from "./hooks/useDnsLocal";

// Hooks — network zones (epic #244)
export {
  useNetworkZones,
  usePendingDevices,
  useCreateNetworkZone,
  useUpdateNetworkZone,
  useDeleteNetworkZone,
  useAssignDeviceZone,
  useQuarantineNewDevices,
  useSetQuarantineNewDevices,
} from "./hooks/useNetworkZones";
export type { PendingDevices } from "./hooks/useNetworkZones";
export {
  useZoneExceptions,
  useCreateZoneException,
  useDeleteZoneException,
} from "./hooks/useZoneExceptions";

export {
  useDnsFilterProfiles,
  useDnsFilterProfile,
  useCreateDnsFilterProfile,
  useUpdateDnsFilterProfile,
  useDeleteDnsFilterProfile,
  useBlocklists,
  useCreateBlocklist,
  useUpdateBlocklist,
  useDeleteBlocklist,
  useRefreshBlocklist,
  useAllowlist,
  useCreateAllowlistEntry,
  useDeleteAllowlistEntry,
  useFilterRules,
  useCreateFilterRule,
  useUpdateFilterRule,
  useDeleteFilterRule,
  useDeviceFilterSettingsList,
  useDeviceFilterSettings,
  useUpdateDeviceFilterSettings,
  useDnsFilterConfig,
  useUpdateDnsFilterConfig,
} from "./hooks/useDnsFilter";

export { useDnsQueryLog } from "./hooks/useDnsLogs";

// Hooks — network + DHCP
export {
  useNetworkStatus,
  useDiscoverGatewayMac,
  useDhcpSelfProbe,
} from "./hooks/useNetwork";
export {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  useUpdateDhcpConfig,
  usePreviewDhcpConfig,
  useCreateReservation,
  useDeleteReservation,
  useRevokeLease,
} from "./hooks/useDhcp";

// Hooks — backup
export {
  useBackupStatus,
  useBackupSnapshots,
  useExportBackup,
  usePreviewImport,
  useApplyImport,
} from "./hooks/useBackup";

// Hooks — updates
export {
  useUpdateStatus,
  useUpdateHistory,
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
} from "./hooks/useUpdate";

// Hooks — setup wizard
export { useSetupStatus, useSetup, useAdvanceWizard } from "./hooks/useSetup";

// Hooks — remote access (DDNS + TLS)
export {
  useRequestEnrollmentCode,
  useEnrollDdns,
  useCheckDdnsSlug,
  useRegisterDdns,
  useConfigureCloudflare,
  useDdnsStatus,
  useTlsStatus,
  useResolutionCheck,
  useDeleteDdns,
} from "./hooks/useRemoteAccess";

// Hooks — inbound WireGuard remote-access grants (issues #809-#813)
export {
  useInboundWgConfig,
  useSetInboundWgConfig,
  useInboundWgPeers,
  useAddInboundWgPeer,
  useRemoveInboundWgPeer,
  useSetInboundWgPeerEnabled,
} from "./hooks/useInboundWg";
export { peerConfigFilename } from "./lib/inboundWgConfig";
export { InboundWgQrCode } from "./components/InboundWgQrCode";
export { InboundWgBetaNotice } from "./components/InboundWgBetaNotice";
export { DnsFilterNotReadyNotice } from "./components/DnsFilterNotReadyNotice";
export { triggerBrowserDownload } from "./lib/download";
export { ModalTitleBlock } from "./components/ModalTitleBlock";
export { AlertModalTitleBlock } from "./components/AlertModalTitleBlock";

// Custom icons (composed marks not in lucide). Exported individually so
// consumers only bundle the ones they import.
export { ShieldWifi } from "./icons/ShieldWifi";
export { GlobeFilter } from "./icons/GlobeFilter";

// PWA helpers
export { registerSW } from "./lib/registerSW";
export type { RegisterSWOptions } from "./lib/registerSW";
export { useInstallPrompt } from "./hooks/useInstallPrompt";
export type { InstallPromptResult } from "./hooks/useInstallPrompt";
export { useOnlineStatus } from "./hooks/useOnlineStatus";
