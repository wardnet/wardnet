import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { toast } from "sonner";
import { RefreshCcwIcon, PowerIcon, LogOutIcon, MonitorIcon, ChevronRightIcon } from "lucide-react";
import { Pill, Text, Heading } from "@wardnet/web";
import {
  useSystemStatus,
  useDaemonStatus,
  useRestart,
  useReboot,
  useAuth,
  useDdnsStatus,
  useTlsStatus,
  useResolutionCheck,
  formatBytes,
  formatDate,
  formatUptime,
} from "@wardnet/web";
import type { DdnsResolutionVerdict } from "@wardnet/js";
import { useOnlineStatusContext } from "@/context/OnlineStatusContext";
import { useBiometric } from "@/hooks/useBiometric";
import { BusyOverlay } from "@/components/BusyOverlay";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { SectionLabel } from "@/components/SectionLabel";
import { Bar } from "@/components/Bar";

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
  const pill = resolution ? verdictPill(resolution.verdict) : null;
  const { showingLastKnownState } = useOnlineStatusContext();
  const { logout } = useAuth();
  const biometric = useBiometric();
  const navigate = useNavigate();
  const restart = useRestart();
  const reboot = useReboot();

  const [confirmAction, setConfirmAction] = useState<"restart" | "reboot" | null>(null);

  useEffect(() => {
    const phase = restart.isOpen ? restart.phase : reboot.isOpen ? reboot.phase : null;
    const active = restart.isOpen ? restart : reboot;
    if (!phase || phase === "idle" || phase === "scheduled" || phase === "down") return;

    if (phase === "ready") {
      const t = setTimeout(() => active.reset(), 1500);
      return () => clearTimeout(t);
    }
    if (phase === "ready_signed_out") {
      logout();
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

  const activePhase = restart.isOpen ? restart.phase : reboot.isOpen ? reboot.phase : null;
  const busyOpen = restart.isOpen || reboot.isOpen;

  const memoryPercent =
    status && status.memory_total_bytes > 0
      ? (status.memory_used_bytes / status.memory_total_bytes) * 100
      : 0;
  const diskPercent =
    status && status.disk_total_bytes > 0
      ? ((status.disk_total_bytes - status.disk_free_bytes) / status.disk_total_bytes) * 100
      : 0;

  return (
    <div className="flex flex-col gap-5 p-4">
      {/* Page header */}
      <div>
        <Heading level={1} size="3xl" weight="bold" className="text-ink">System</Heading>
        <Text as="p" size="base" className="text-ink-3">Daemon health, power and alerts.</Text>
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
                <Text as="span" size="lg" weight="semibold" className="text-ink">Wardnet</Text>
                <div className="ml-auto">
                  <Pill variant={daemonStatus?.reachable ? "ok" : "warn"} data-testid="system-status-pill">
                    <span className="mr-1" aria-hidden>●</span>
                    {daemonStatus?.reachable ? "Running" : "Unreachable"}
                  </Pill>
                </div>
              </div>

              {/* 2×2 metric grid */}
              <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                <div>
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                    Version
                  </Text>
                  <Text as="p" size="base" className="mt-1 font-mono text-ink">
                    {daemonStatus?.version ? `v${daemonStatus.version}` : "—"}
                  </Text>
                </div>
                <div>
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                    Uptime
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status ? formatUptime(status.uptime_seconds) : "—"}
                  </Text>
                </div>
                <div>
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                    CPU
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status ? `${status.cpu_usage_percent.toFixed(1)}%` : "—"}
                  </Text>
                  {status && <Bar percent={status.cpu_usage_percent} />}
                </div>
                <div>
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                    Memory
                  </Text>
                  <Text as="p" size="base" className="mt-1 text-ink">
                    {status
                      ? `${formatBytes(status.memory_used_bytes)} / ${formatBytes(status.memory_total_bytes)}`
                      : "—"}
                  </Text>
                  {status && status.memory_total_bytes > 0 && <Bar percent={memoryPercent} />}
                </div>
              </div>

              {/* Disk — full-width below the grid. Same label → value → bar
                  stack as the CPU/Memory tiles above. */}
              {status && status.disk_total_bytes > 0 && (
                <div className="mt-4 border-t border-line pt-4">
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
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
                  <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                    Domain
                  </Text>
                  <Text as="p" size="sm" className="mt-1 break-all font-mono text-ink">
                    {ddns?.fqdn ?? "—"}
                  </Text>
                </div>

                <div className="mt-4 grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line pt-4">
                  <div>
                    <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                      Public DNS
                    </Text>
                    <div className="mt-1">
                      {pill ? (
                        <Pill variant={pill.variant}>{pill.label}</Pill>
                      ) : (
                        <Text as="span" size="base" className="text-ink">—</Text>
                      )}
                    </div>
                  </div>
                  <div>
                    <Text as="p" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
                      Certificate
                    </Text>
                    <Text as="p" size="base" className="mt-1 text-ink">
                      {tls?.not_after
                        ? `Until ${formatDate(tls.not_after)}`
                        : tls?.phase === "issuing"
                          ? "Issuing…"
                          : "—"}
                    </Text>
                  </div>
                </div>
              </div>
            </div>
          )}

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
                  <Text as="p" size="base" weight="semibold" className="text-ink">Restart daemon</Text>
                  <Text as="p" size="xs" className="text-ink-3">Reloads Wardnet · ~5s downtime</Text>
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
                  <Text as="p" size="base" weight="semibold" className="text-danger">Reboot device</Text>
                  <Text as="p" size="xs" className="text-danger opacity-60">Full restart · ~60s offline</Text>
                </div>
                <ChevronRightIcon size={16} className="shrink-0 text-danger opacity-40" />
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
              <Text as="p" size="base" weight="semibold" className="text-ink">Open desktop admin</Text>
              <Text as="p" size="xs" className="text-ink-3">Full management interface</Text>
            </div>
            <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
          </a>
          <button
            data-testid="system-logout"
            onClick={() => {
              logout();
              biometric.unregister();
              navigate("/login", { replace: true });
            }}
            className="flex w-full items-center gap-3 rounded-xl border border-line bg-card px-4 py-3.5 text-left active:bg-sunken"
          >
            <div className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-sunken">
              <LogOutIcon size={18} className="text-ink-2" />
            </div>
            <div className="min-w-0 flex-1">
              <Text as="p" size="base" weight="semibold" className="text-ink">Log out</Text>
              <Text as="p" size="xs" className="text-ink-3">Sign out of this device</Text>
            </div>
            <ChevronRightIcon size={16} className="shrink-0 text-ink-4" />
          </button>
        </div>
      </div>

      {/* Confirm dialogs */}
      <ConfirmDialog
        open={confirmAction === "restart"}
        onOpenChange={(open) => !open && setConfirmAction(null)}
        onConfirm={() => { setConfirmAction(null); restart.start(); }}
        title="Restart daemon?"
        description="The daemon will restart. You'll stay logged in and the page will reconnect automatically."
        confirmLabel="Restart"
        variant="warn"
      />
      <ConfirmDialog
        open={confirmAction === "reboot"}
        onOpenChange={(open) => !open && setConfirmAction(null)}
        onConfirm={() => { setConfirmAction(null); reboot.start(); }}
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
            activePhase === "ready" || activePhase === "ready_signed_out" ? "done" : "working"
          }
        />
      )}
    </div>
  );
}
