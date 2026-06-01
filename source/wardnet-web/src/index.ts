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
  formatUptime,
  formatDate,
  formatTime,
  formatTimeShort,
  formatDateTime,
  timeAgo,
  apiErrorMessage,
  apiRequestId,
} from "./lib/utils";

// Country / device helpers
export { countryFlag } from "./lib/country";
export { DEVICE_TYPE_OPTIONS, deviceTypeLabel } from "./lib/device";
export { cronToHuman } from "./lib/cron";
export { logger, createLogger, setLevel } from "./lib/logger";
export type { Logger, LogLevel } from "./lib/logger";

// Auth store
export { useAuthStore } from "./stores/authStore";

// Components
export { JobProgressDescription } from "./components/JobProgressDescription";
export { LoginForm } from "./components/LoginForm";

// Hooks — auth
export { useAuth } from "./hooks/useAuth";

// Hooks — devices
export { useDevices, useDevice, useMyDevice, useSetMyRule, useUpdateDevice } from "./hooks/useDevices";

// Hooks — tunnels
export {
  useTunnels,
  useTunnel,
  useTunnelDevices,
  useCreateTunnel,
  useDeleteTunnel,
  useTestTunnel,
  useSetTunnelDnsOverride,
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

export { useDefaultPolicy, useSetDefaultPolicy } from "./hooks/useDefaultPolicy";

// Hooks — daemon lifecycle
export { useDaemonStatus } from "./hooks/useDaemonStatus";
export {
  useDaemonReachability,
} from "./hooks/useDaemonReachability";
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
  RANGES,
  RANGE_HOURS,
} from "./hooks/useStats";
export type { StatsRange, DnsStatsDashboardData } from "./hooks/useStats";
export { useTunnelStats } from "./hooks/useTunnelStats";
export type { TunnelStatsPoint, TunnelStatsData } from "./hooks/useTunnelStats";

// Hooks — DNS
export {
  useDnsStatus,
  useDnsConfig,
  useToggleDns,
  useUpdateDnsConfig,
  useFlushDnsCache,
} from "./hooks/useDns";

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
export { useNetworkStatus, useDiscoverGatewayMac, useDhcpSelfProbe } from "./hooks/useNetwork";
export {
  useDhcpStatus,
  useDhcpConfig,
  useDhcpLeases,
  useDhcpReservations,
  useToggleDhcp,
  useUpdateDhcpConfig,
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

// PWA helpers
export { registerSW } from "./lib/registerSW";
export type { RegisterSWOptions } from "./lib/registerSW";
export { useInstallPrompt } from "./hooks/useInstallPrompt";
export type { InstallPromptResult } from "./hooks/useInstallPrompt";
export { useOnlineStatus } from "./hooks/useOnlineStatus";
