import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { PageHeader } from "@/components/compound/PageHeader";
import { UpdateCard } from "@/components/features/UpdateCard";
import { useSystemStatus } from "@/hooks/useSystemStatus";
import {
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
  useUpdateStatus,
} from "@/hooks/useUpdate";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Settings page for system configuration (admin only).
 *
 *  Power controls live at `/power`, backup & restore live at
 *  `/backups`. Settings now carries system information, updates,
 *  and (future) account management — the things that don't fit
 *  into a dedicated sibling page. */
export default function Settings() {
  const { data: status, isLoading } = useSystemStatus();
  const { data: updateStatus, isLoading: updateLoading } = useUpdateStatus();
  const check = useCheckForUpdates();
  const install = useInstallUpdate();
  const rollback = useRollbackUpdate();
  const saveConfig = useUpdateConfig();

  return (
    <div className="col gap-20">
      <PageHeader
        title="Settings"
        description="System information, software updates, and account management."
      />

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
  );
}
