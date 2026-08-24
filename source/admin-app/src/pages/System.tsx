import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "@wardnet/ui";
import {
  RefreshCcwIcon,
  PowerIcon,
  LogOutIcon,
  MonitorIcon,
  ChevronRightIcon,
  BellIcon,
} from "lucide-react";
import { Pill, Text, Heading, Toggle } from "@wardnet/web";
import {
  useSystemStatus,
  useDaemonStatus,
  useRestart,
  useReboot,
  useAuth,
  useDdnsStatus,
  useTlsStatus,
  useResolutionCheck,
  useInboundWgConfig,
  useSetInboundWgConfig,
  usePushNotifications,
  useRecentNotifications,
  useClearNotifications,
  isIosBrowserTab,
  formatBytes,
  formatDate,
  formatUptime,
  timeAgo,
} from "@wardnet/web";
import type { DdnsResolutionVerdict, NotificationItem } from "@wardnet/js";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { useBiometric } from "@/hooks/useBiometric";
import { BusyOverlay } from "@/components/BusyOverlay";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { AnomaliesList } from "@/components/AnomaliesList";
import { SectionLabel } from "@/components/SectionLabel";
import { Bar } from "@/components/Bar";

/** Pill variant + short label for a notification-feed entry's kind. */
function kindPill(kind: NotificationItem["kind"]): {
  variant: "ok" | "warn" | "down" | "info";
  label: string;
} {
  switch (kind) {
    // Anomaly-backed entries carry the anomaly type's own slug as the kind,
    // not a generic "anomaly", so each needs a case here — without one a
    // tunnel going down falls to the "System" default and reads as routine
    // noise.
    //
    // They are deliberately `info` rather than severity-coloured. The feed is
    // a *history* holding both edges of an anomaly, and the opened and
    // resolved notifications share a kind: `notify_anomaly_resolved` marks the
    // difference with `data.state`, which is never persisted (neither
    // `StoredNotification` nor the SDK's `NotificationItem` carries it). So a
    // row here cannot be known to be a failure, and colouring it red would
    // paint "Problem resolved" as an outage. Severity belongs to the
    // Anomalies card above, which lists only what is currently open and reads
    // it from `anomaly.severity`.
    //
    // The label still names the subsystem, which is what the original bug
    // cost. Wording mirrors the anomaly's `component`, so both blocks on this
    // screen use one vocabulary.
    case "tunnel_start_failed":
    case "tunnel_unhealthy":
      return { variant: "info", label: "Tunnel" };
    case "route_table_lost":
      return { variant: "info", label: "Routing" };
    case "blocklist_refresh_failing":
      return { variant: "info", label: "DNS" };
    case "dhcp_conflict":
      return { variant: "info", label: "DHCP" };
    case "update_failed":
      return { variant: "info", label: "Update" };
    // Retired kind, kept for historic rows. Unlike the anomaly kinds it is
    // unambiguously a failure — nothing ever published a recovery under it —
    // so it keeps its colour.
    case "tunnel_offline":
      return { variant: "down", label: "Tunnel" };
    case "new_device_quarantined":
      return { variant: "warn", label: "New device" };
    case "routing_changed":
      return { variant: "info", label: "Routing" };
    // `rule_request_created` is the pre-#919 wire string. Feed rows persist, so
    // entries written before the rename would otherwise fall through to the
    // generic "System" pill and silently relabel a household member's ask.
    case "access_request_created":
    case "rule_request_created":
      return { variant: "info", label: "Request" };
    default:
      return { variant: "info", label: "System" };
  }
}

/**
 * Notifications section (issue #482): enable/disable Web Push for this
 * browser plus the daemon-persisted recent-notifications feed. The feed is
 * shared across admin accounts; Clear empties it for everyone.
 */
function NotificationsSection() {
  const push = usePushNotifications();
  const { data: notifications } = useRecentNotifications();
  const clear = useClearNotifications();

  const helperText =
    push.state === "denied"
      ? "Notifications are blocked in your browser settings."
      : push.state === "unsupported"
        ? isIosBrowserTab()
          ? "Install the app to your Home Screen to enable notifications."
          : "Notifications are not supported in this browser."
        : "Tunnel and device alerts, even when the app is closed";

  return (
    <div>
      <SectionLabel>Notifications</SectionLabel>
      <div className="rounded-xl border border-line bg-card">
        <div className="flex items-center gap-3 px-4 py-3.5">
          <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
            <BellIcon size={18} className="text-ink-2" />
          </div>
          <div className="min-w-0 flex-1">
            <Text as="p" size="base" weight="semibold" className="text-ink">
              Push notifications
            </Text>
            <Text as="p" size="xs" className="text-ink-3">
              {helperText}
            </Text>
          </div>
          <Toggle
            checked={push.state === "subscribed"}
            onCheckedChange={(checked: boolean) =>
              checked ? void push.subscribe() : void push.unsubscribe()
            }
            disabled={
              push.state === "unsupported" ||
              push.state === "denied" ||
              push.isBusy
            }
            aria-label="Toggle push notifications"
            data-testid="system-notifications-toggle"
          />
        </div>

        {notifications && notifications.length > 0 && (
          <div
            className="border-t border-line"
            data-testid="system-notifications-feed"
          >
            <ul className="divide-y divide-line">
              {notifications.map((n) => {
                const pill = kindPill(n.kind);
                return (
                  <li key={n.id} className="flex items-start gap-3 px-4 py-3">
                    <div className="mt-0.5 shrink-0">
                      <Pill variant={pill.variant}>{pill.label}</Pill>
                    </div>
                    <div className="min-w-0 flex-1">
                      <Text
                        as="p"
                        size="sm"
                        weight="semibold"
                        className="text-ink"
                      >
                        {n.title}
                      </Text>
                      <Text as="p" size="xs" className="text-ink-3">
                        {n.body}
                      </Text>
                    </div>
                    <Text
                      as="span"
                      size="2xs"
                      className="mt-0.5 shrink-0 whitespace-nowrap text-ink-4"
                    >
                      {timeAgo(n.created_at)}
                    </Text>
                  </li>
                );
              })}
            </ul>
            <div className="border-t border-line px-4 py-2.5">
              <button
                data-testid="system-notifications-clear"
                onClick={() => clear.mutate()}
                disabled={clear.isPending}
                className="text-left"
              >
                <Text
                  as="span"
                  size="xs"
                  weight="semibold"
                  className="text-ink-3"
                >
                  Clear all notifications
                </Text>
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

/** Pill variant + label for the read-only resolution verdict. */
function verdictPill(verdict: DdnsResolutionVerdict): {
  variant: "ok" | "warn" | "down" | "info";
  label: string;
} {
  switch (verdict) {
    case "match":
      return { variant: "ok", label: "Reachable" };
    case "mismatch":
      return { variant: "down", label: "Wrong IP" };
    case "pending":
      return { variant: "warn", label: "Propagating" };
    case "not_configured":
      return { variant: "info", label: "Not set up" };
  }
}

export default function System() {
  const { data: status } = useSystemStatus();
  const { data: daemonStatus } = useDaemonStatus();
  const { data: ddns } = useDdnsStatus();
  const secureEnabled = !!ddns?.provider;
  const { data: tls } = useTlsStatus({ enabled: secureEnabled });
  const { data: resolution } = useResolutionCheck(secureEnabled);
  const { data: inboundWg } = useInboundWgConfig();
  const setInboundWgConfig = useSetInboundWgConfig();
  const pill = resolution ? verdictPill(resolution.verdict) : null;
  const { showingLastKnownState } = useOnlineStatusContext();
  const { logout } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();
  const restart = useRestart();
  const reboot = useReboot();

  const [confirmAction, setConfirmAction] = useState<
    "restart" | "reboot" | null
  >(null);

  useEffect(() => {
    const phase = restart.isOpen
      ? restart.phase
      : reboot.isOpen
        ? reboot.phase
        : null;
    const active = restart.isOpen ? restart : reboot;
    if (!phase || phase === "idle" || phase === "scheduled" || phase === "down")
      return;

    if (phase === "ready") {
      const t = setTimeout(() => active.reset(), 1500);
      return () => clearTimeout(t);
    }
    if (phase === "ready_signed_out") {
      // The reboot already invalidated the session server-side; logout() is
      // fire-and-forget here — it clears local auth state regardless of
      // whether the (already-dead) session revoke round-trip succeeds.
      void logout();
      biometric.unregister();
      active.reset();
      navigate("/login");
      return;
    }
    if (phase === "did_not_fire" || phase === "timeout" || phase === "failed") {
      toast.error(active.errorMessage ?? "Action failed");
      active.reset();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [restart.phase, reboot.phase]);

  const activePhase = restart.isOpen
    ? restart.phase
    : reboot.isOpen
      ? reboot.phase
      : null;
  const busyOpen = restart.isOpen || reboot.isOpen;

  const memoryPercent =
    status && status.memory_total_bytes > 0
      ? (status.memory_used_bytes / status.memory_total_bytes) * 100
      : 0;
  const diskPercent =
    status && status.disk_total_bytes > 0
      ? ((status.disk_total_bytes - status.disk_free_bytes) /
          status.disk_total_bytes) *
        100
      : 0;

  return (
    <div className="flex flex-col gap-5 p-4">
      {/* Page header */}
      <div>
        <Heading level={1} size="3xl" weight="bold" className="text-ink">
          System
        </Heading>
        <Text as="p" size="base" className="text-ink-3">
          Daemon health, power and alerts.
        </Text>
      </div>

      <div
        className={
          showingLastKnownState
            ? "pointer-events-none opacity-40 transition-opacity"
            : "transition-opacity"
        }
      >
        <div className="flex flex-col gap-5">
          {/* ── Daemon section ── */}
          <div>
            <SectionLabel>Daemon</SectionLabel>
            <div className="rounded-xl border border-line bg-card p-4">
              {/* Identity + status pill */}
              <div className="flex items-center gap-2.5">
                <span
                  className={`size-2 shrink-0 rounded-full ${daemonStatus?.reachable ? "bg-accent" : "bg-warn"}`}
                />
                <Text
                  as="span"
                  size="lg"
                  weight="semibold"
                  className="text-ink"
                >
                  Wardnet
                </Text>
                <div className="ml-auto">
                  <Pill
                    variant={daemonStatus?.reachable ? "ok" : "warn"}
                    data-testid="system-status-pill"
                  >
                    <span className="mr-1" aria-hidden>
                      ●
                    </span>
                    {daemonStatus?.reachable ? "Running" : "Unreachable"}
                  </Pill>
                </div>
              </div>

              {/* 2×2 metric grid */}
              <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Version
                  </Text>
                  <Text as="p" size="base" className="mt-1 font-mono text-ink">
                    {daemonStatus?.version ? `v${daemonStatus.version}` : "-"}
                  </Text>
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Uptime
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status ? formatUptime(status.uptime_seconds) : "-"}
                  </Text>
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    CPU
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status ? `${status.cpu_usage_percent.toFixed(1)}%` : "-"}
                  </Text>
                  {status && <Bar percent={status.cpu_usage_percent} />}
                </div>
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Memory
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status
                      ? `${formatBytes(status.memory_used_bytes)} / ${formatBytes(status.memory_total_bytes)}`
                      : "-"}
                  </Text>
                  {status && status.memory_total_bytes > 0 && (
                    <Bar percent={memoryPercent} />
                  )}
                </div>
              </div>

              {/* Disk — full-width below the grid. Same label → value → bar
                  stack as the CPU/Memory tiles above. */}
              {status && status.disk_total_bytes > 0 && (
                <div className="mt-4 border-t border-line pt-4">
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Disk
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {formatBytes(status.disk_free_bytes)} free of{" "}
                    {formatBytes(status.disk_total_bytes)}
                  </Text>
                  <Bar percent={diskPercent} />
                </div>
              )}
            </div>
          </div>

          {/* ── Remote access section (read-only) ── */}
          {secureEnabled && (
            <div>
              <SectionLabel>Remote access</SectionLabel>
              <div className="rounded-xl border border-line bg-card p-4">
                <div>
                  <Text
                    as="p"
                    size="2xs"
                    weight="semibold"
                    className="uppercase tracking-wider text-ink-3"
                  >
                    Domain
                  </Text>
                  <Text
                    as="p"
                    size="sm"
                    className="mt-1 break-all font-mono text-ink"
                  >
                    {ddns?.fqdn ?? "-"}
                  </Text>
                </div>

                <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                  <div>
                    <Text
                      as="p"
                      size="2xs"
                      weight="semibold"
                      className="uppercase tracking-wider text-ink-3"
                    >
                      Public DNS
                    </Text>
                    <div className="mt-1">
                      {pill ? (
                        <Pill variant={pill.variant}>{pill.label}</Pill>
                      ) : (
                        <Text as="span" size="base" className="text-ink">
                          -
                        </Text>
                      )}
                    </div>
                  </div>
                  <div>
                    <Text
                      as="p"
                      size="2xs"
                      weight="semibold"
                      className="uppercase tracking-wider text-ink-3"
                    >
                      Certificate
                    </Text>
                    <Text as="p" size="base" className="mt-1 text-ink">
                      {tls?.not_after
                        ? `Until ${formatDate(tls.not_after)}`
                        : tls?.phase === "issuing"
                          ? "Issuing…"
                          : "-"}
                    </Text>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ── VPN section (issue #813) ── */}
          <div>
            <SectionLabel>VPN</SectionLabel>
            <div className="rounded-xl border border-line bg-card p-4">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <Text as="p" size="base" weight="medium" className="text-ink">
                    Inbound WireGuard server
                  </Text>
                  <Text as="p" size="xs" className="mt-0.5 text-ink-3">
                    {inboundWg?.enabled
                      ? `Listening on port ${inboundWg.listen_port}`
                      : "Lets granted devices connect back in from off the LAN"}
                  </Text>
                </div>
                <Toggle
                  aria-label="Enable inbound WireGuard server"
                  checked={inboundWg?.enabled ?? false}
                  disabled={setInboundWgConfig.isPending}
                  onCheckedChange={(next) =>
                    setInboundWgConfig.mutate({
                      enabled: next,
                      listen_port: inboundWg?.listen_port ?? 51821,
                    })
                  }
                />
              </div>
            </div>
          </div>

          {/* ── Anomalies: what is wrong right now, above the feed of
                 what has happened. Renders nothing when all is well. ── */}
          <AnomaliesList />

          {/* ── Notifications section (issue #482) ── */}
          <NotificationsSection />

          {/* ── Power section ── */}
          <div>
            <SectionLabel>Power</SectionLabel>
            <div className="flex flex-col gap-2">
              <button
                data-testid="system-restart-daemon"
                onClick={() => setConfirmAction("restart")}
                className="flex w-full items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5 text-left active:bg-sunken"
              >
                <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
                  <RefreshCcwIcon size={18} className="text-ink-2" />
                </div>
                <div className="min-w-0 flex-1">
                  <Text
                    as="p"
                    size="base"
                    weight="semibold"
                    className="text-ink"
                  >
                    Restart daemon
                  </Text>
                  <Text as="p" size="xs" className="text-ink-3">
                    Reloads Wardnet · ~5s downtime
                  </Text>
                </div>
                <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
              </button>

              <button
                data-testid="system-reboot-device"
                onClick={() => setConfirmAction("reboot")}
                className="flex w-full items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5 text-left active:bg-sunken"
              >
                <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-danger-soft">
                  <PowerIcon size={18} className="text-danger" />
                </div>
                <div className="min-w-0 flex-1">
                  <Text
                    as="p"
                    size="base"
                    weight="semibold"
                    className="text-danger"
                  >
                    Reboot device
                  </Text>
                  <Text as="p" size="xs" className="text-danger opacity-60">
                    Full restart · ~60s offline
                  </Text>
                </div>
                <ChevronRightIcon
                  size={16}
                  className="shrink-0 text-danger opacity-40"
                />
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* ── Account section — always interactive ── */}
      <div>
        <SectionLabel>Account</SectionLabel>
        <div className="flex flex-col gap-2">
          <a
            href="/admin/"
            data-testid="system-open-desktop"
            className="flex w-full items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5 active:bg-sunken"
          >
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
              <MonitorIcon size={18} className="text-ink-2" />
            </div>
            <div className="min-w-0 flex-1">
              <Text as="p" size="base" weight="semibold" className="text-ink">
                Open desktop admin
              </Text>
              <Text as="p" size="xs" className="text-ink-3">
                Full management interface
              </Text>
            </div>
            <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
          </a>
          <button
            data-testid="system-logout"
            onClick={() => {
              // Revoke the server session first; only once that settles is
              // the biometric credential (the last local gate) dropped and
              // the user routed away. Local auth state clears synchronously
              // inside logout(), and the request is time-boxed by the SDK,
              // so this never strands the UI.
              void logout().then((revoked) => {
                if (!revoked) {
                  toast.error(
                    "Couldn't reach the gateway — sign-out may not have completed there",
                  );
                }
                biometric.unregister();
                navigate("/login", { replace: true });
              });
            }}
            className="flex w-full items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5 text-left active:bg-sunken"
          >
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
              <LogOutIcon size={18} className="text-ink-2" />
            </div>
            <div className="min-w-0 flex-1">
              <Text as="p" size="base" weight="semibold" className="text-ink">
                Log out
              </Text>
              <Text as="p" size="xs" className="text-ink-3">
                Sign out of this device
              </Text>
            </div>
            <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
          </button>
        </div>
      </div>

      {/* Confirm dialogs */}
      <ConfirmDialog
        open={confirmAction === "restart"}
        onOpenChange={(open) => !open && setConfirmAction(null)}
        onConfirm={() => {
          setConfirmAction(null);
          restart.start();
        }}
        title="Restart daemon?"
        description="The daemon will restart. You'll stay logged in and the page will reconnect automatically."
        confirmLabel="Restart"
        variant="warn"
      />
      <ConfirmDialog
        open={confirmAction === "reboot"}
        onOpenChange={(open) => !open && setConfirmAction(null)}
        onConfirm={() => {
          setConfirmAction(null);
          reboot.start();
        }}
        title="Reboot device?"
        description="The device will reboot fully. This takes about 60 seconds. You may need to log in again."
        confirmLabel="Reboot"
        variant="danger"
      />

      {/* Busy overlay */}
      {busyOpen && (
        <BusyOverlay
          action={restart.isOpen ? "restart" : "reboot"}
          phase={
            activePhase === "ready" || activePhase === "ready_signed_out"
              ? "done"
              : "working"
          }
        />
      )}
    </div>
  );
}
