import { useState } from "react";
import { useNavigate } from "react-router";
import type { RestorePreviewResponse } from "@wardnet/js";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { PageHeader } from "@/components/compound/PageHeader";
import { BackupCard } from "@/components/features/BackupCard";
import { DnsFilterSettingsCard } from "@/components/features/DnsFilterSettingsCard";
import { PowerCard } from "@/components/features/PowerCard";
import { RestartProgressDialog } from "@/components/features/RestartProgressDialog";
import { ShutdownProgressDialog } from "@/components/features/ShutdownProgressDialog";
import { UpdateCard } from "@/components/features/UpdateCard";
import { useApplyImport, useExportBackup, usePreviewImport } from "@/hooks/useBackup";
import { useReboot } from "@/hooks/useReboot";
import { useRestart } from "@/hooks/useRestart";
import { useShutdown } from "@/hooks/useShutdown";
import { useSystemStatus } from "@/hooks/useSystemStatus";
import {
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
  useUpdateStatus,
} from "@/hooks/useUpdate";
import { useAuthStore } from "@/stores/authStore";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Settings page for system configuration (admin only). */
export default function Settings() {
  const { data: status, isLoading } = useSystemStatus();
  const { data: updateStatus, isLoading: updateLoading } = useUpdateStatus();
  const check = useCheckForUpdates();
  const install = useInstallUpdate();
  const rollback = useRollbackUpdate();
  const saveConfig = useUpdateConfig();
  const navigate = useNavigate();
  const logout = useAuthStore((s) => s.logout);

  // Backup flow — preview response is held in page-local state so the
  // BackupCard can render it between the two steps of the restore
  // wizard. `onDismissPreview` clears it when the operator cancels.
  const [preview, setPreview] = useState<RestorePreviewResponse | null>(null);
  const exportBackup = useExportBackup();
  const previewImport = usePreviewImport();
  const applyImport = useApplyImport();

  // Power lifecycles — three independent hooks, one shared poll
  // implementation under the hood (see `useDaemonReachability`).
  const restart = useRestart();
  const reboot = useReboot();
  const shutdown = useShutdown();

  const anyPowerOpInFlight = restart.isOpen || reboot.isOpen || shutdown.isOpen;

  const onRestartReady = () => {
    restart.reset();
  };
  const onRestartSignIn = () => {
    // Session cookie was invalidated by the restart (e.g. in-memory
    // session store). Clear local admin flag and route to login.
    logout();
    restart.reset();
    void navigate("/login");
  };

  const onRebootReady = () => {
    reboot.reset();
  };
  const onRebootSignIn = () => {
    logout();
    reboot.reset();
    void navigate("/login");
  };

  return (
    <>
      <PageHeader title="Settings" />
      <div className="flex flex-col gap-4">
        <Card>
          <CardHeader>
            <CardTitle>System information</CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <p className="text-sm text-ink-3">Loading…</p>
            ) : status ? (
              <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
                <div>
                  <dt className="text-ink-3">Version</dt>
                  <dd className="font-medium" title={`build: ${status.version}`}>
                    {status.release_version}
                  </dd>
                </div>
                <div>
                  <dt className="text-ink-3">Uptime</dt>
                  <dd className="font-medium">{formatUptime(status.uptime_seconds)}</dd>
                </div>
                <div>
                  <dt className="text-ink-3">Devices</dt>
                  <dd className="font-medium">{status.device_count}</dd>
                </div>
                <div>
                  <dt className="text-ink-3">Tunnels</dt>
                  <dd className="font-medium">{status.tunnel_count}</dd>
                </div>
                <div>
                  <dt className="text-ink-3">Database size</dt>
                  <dd className="font-medium">{formatBytes(status.db_size_bytes)}</dd>
                </div>
              </dl>
            ) : (
              <p className="text-sm text-ink-3">Unable to connect to daemon.</p>
            )}
          </CardContent>
        </Card>

        <PowerCard
          onReboot={() => reboot.start()}
          onShutdown={() => shutdown.start()}
          onRestartDaemon={() => restart.start()}
          busy={anyPowerOpInFlight}
        />

        <UpdateCard
          status={updateStatus?.status ?? null}
          isLoading={updateLoading}
          isChecking={check.isPending}
          isInstalling={install.isPending}
          isRollingBack={rollback.isPending}
          onCheck={() => check.mutate()}
          onInstall={() => install.mutate({})}
          onRollback={() => rollback.mutate()}
          onToggleAutoUpdate={(enabled) => saveConfig.mutate({ auto_update_enabled: enabled })}
          onChangeChannel={(channel) => saveConfig.mutate({ channel })}
        />

        <DnsFilterSettingsCard />

        <BackupCard
          isExporting={exportBackup.isPending}
          isPreviewing={previewImport.isPending}
          isApplying={applyImport.isPending}
          preview={preview}
          onExport={(passphrase) => exportBackup.mutate({ passphrase })}
          onPreview={(args) =>
            previewImport.mutate(args, {
              onSuccess: (data) => setPreview(data),
            })
          }
          onApply={(previewToken) =>
            applyImport.mutate(
              { preview_token: previewToken },
              {
                onSuccess: () => {
                  setPreview(null);
                  // Reuse the same lifecycle as the manual restart;
                  // the dialog will walk through scheduled → down →
                  // ready and offer a sign-in button if needed.
                  restart.start();
                },
              },
            )
          }
          onDismissPreview={() => setPreview(null)}
        />

        <Card>
          <CardHeader>
            <CardTitle>Account</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-ink-3">
              Account management will be available in a future release.
            </p>
          </CardContent>
        </Card>
      </div>

      {/* Restart-daemon flow. Same dialog the post-restore prompt uses. */}
      <RestartProgressDialog
        open={restart.isOpen}
        phase={restart.phase}
        startedAt={restart.startedAt}
        errorMessage={restart.errorMessage}
        onDismiss={onRestartReady}
        onSignIn={onRestartSignIn}
      />

      {/* Safe-Reboot flow — lifecycle is the same shape as restart
          (the daemon is expected to come back), so we re-use the
          same progress dialog. The copy is generic enough to fit
          both. */}
      <RestartProgressDialog
        open={reboot.isOpen}
        phase={reboot.phase}
        startedAt={reboot.startedAt}
        errorMessage={reboot.errorMessage}
        onDismiss={onRebootReady}
        onSignIn={onRebootSignIn}
      />

      {/* Safe-Shutdown flow — distinct dialog because the terminal
          state is "host is off, no recovery" rather than "back up". */}
      <ShutdownProgressDialog
        open={shutdown.isOpen}
        phase={shutdown.phase}
        startedAt={shutdown.startedAt}
        errorMessage={shutdown.errorMessage}
        onDismiss={() => shutdown.reset()}
      />
    </>
  );
}
