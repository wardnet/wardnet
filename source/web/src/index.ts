// UI Primitives (merged from @wardnet/forge-web)
export {
  AlertModal,
  AlertModalTrigger,
  AlertModalContent,
  AlertModalHeader,
  AlertModalTitle,
  AlertModalDescription,
  AlertModalBody,
  AlertModalFooter,
  AlertModalAction,
  AlertModalCancel,
} from "./primitives/alert-modal";
export type {
  AlertModalProps,
  AlertModalTriggerProps,
  AlertModalContentProps,
  AlertModalTitleProps,
  AlertModalDescriptionProps,
  AlertModalActionProps,
  AlertModalCancelProps,
} from "./primitives/alert-modal";
export { Banner } from "./primitives/banner";
export type { BannerProps, BannerTone } from "./primitives/banner";
export { Button } from "./primitives/button";
export type { ButtonProps } from "./primitives/button";
export {
  Card,
  CardHeader,
  CardTitle,
  CardSubtitle,
  CardAction,
  CardContent,
  CardFooter,
} from "./primitives/card";
export { Combobox, ComboboxItem } from "./primitives/combobox";
export type { ComboboxProps, ComboboxItemProps } from "./primitives/combobox";
export {
  Drawer,
  DrawerTrigger,
  DrawerContent,
  DrawerTitle,
  DrawerDescription,
  DrawerClose,
} from "./primitives/drawer";
export type {
  DrawerProps,
  DrawerTriggerProps,
  DrawerContentProps,
  DrawerTitleProps,
  DrawerDescriptionProps,
  DrawerCloseProps,
} from "./primitives/drawer";
export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "./primitives/dropdown-menu";
export type {
  DropdownMenuProps,
  DropdownMenuTriggerProps,
  DropdownMenuContentProps,
  DropdownMenuItemProps,
  DropdownMenuSeparatorProps,
} from "./primitives/dropdown-menu";
export { Field } from "./primitives/field";
export type { FieldProps } from "./primitives/field";
export {
  Form,
  Validator,
  FormContext,
  useFormContext,
} from "./primitives/form";
export type { Validate } from "./primitives/form";
export { Input } from "./primitives/input";
export type { InputProps } from "./primitives/input";
export { Label } from "./primitives/label";
export type { LabelProps } from "./primitives/label";
export {
  Modal,
  ModalTrigger,
  ModalContent,
  ModalHeader,
  ModalTitle,
  ModalDescription,
  ModalBody,
  ModalFooter,
  ModalClose,
} from "./primitives/modal";
export type {
  ModalProps,
  ModalTriggerProps,
  ModalContentProps,
} from "./primitives/modal";
export { Pill } from "./primitives/pill";
export type { PillProps } from "./primitives/pill";
export { Popover, PopoverTrigger, PopoverContent } from "./primitives/popover";
export type {
  PopoverProps,
  PopoverTriggerProps,
  PopoverContentProps,
} from "./primitives/popover";
export {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "./primitives/select";
export { Sparkline } from "./primitives/sparkline";
export type { SparklineProps } from "./primitives/sparkline";
export { StatTile } from "./primitives/stat-tile";
export type { StatTileProps } from "./primitives/stat-tile";
export { Tabs, TabsList, TabsTrigger, TabsContent } from "./primitives/tabs";
export type {
  TabsProps,
  TabsListProps,
  TabsTriggerProps,
  TabsContentProps,
} from "./primitives/tabs";
export { Textarea } from "./primitives/textarea";
export type { TextareaProps } from "./primitives/textarea";
export { Toggle } from "./primitives/toggle";
export type { ToggleProps } from "./primitives/toggle";

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
export { DeviceIcon } from "./components/DeviceIcon";
export { JobProgressDescription } from "./components/JobProgressDescription";
export { LoginForm } from "./components/LoginForm";
export { RoutingSelector } from "./components/RoutingSelector";

// Hooks — auth
export { useAuth } from "./hooks/useAuth";

// Hooks — devices
export {
  useDevices,
  useDevice,
  useMyDevice,
  useSetMyRule,
  useUpdateDevice,
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
} from "./hooks/useDevices";

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
  useCheckDdnsName,
  useRegisterDdns,
  useConfigureCloudflare,
  useDdnsStatus,
  useTlsStatus,
  useResolutionCheck,
  useDeleteDdns,
} from "./hooks/useRemoteAccess";

// PWA helpers
export { registerSW } from "./lib/registerSW";
export type { RegisterSWOptions } from "./lib/registerSW";
export { useInstallPrompt } from "./hooks/useInstallPrompt";
export type { InstallPromptResult } from "./hooks/useInstallPrompt";
export { useOnlineStatus } from "./hooks/useOnlineStatus";
