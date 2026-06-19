import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/web";
import { Text } from "@wardnet/web";
import { PageHeader } from "@/components/compound/PageHeader";
import { UpdateCard } from "@/components/features/UpdateCard";
import { useSystemStatus } from "@wardnet/web";
import {
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
  useUpdateStatus,
} from "@wardnet/web";
import { formatBytes, formatUptime } from "@wardnet/web";

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
            <Text as="p" size="sm" className="text-ink-3">
              Loading…
            </Text>
          ) : status ? (
            <Text
              as="dl"
              size="sm"
              className="grid grid-cols-2 gap-x-8 gap-y-3 sm:grid-cols-3"
            >
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Version
                </Text>
                <Text
                  as="dd"
                  weight="medium"
                  title={`build: ${status.version}`}
                >
                  {status.release_version}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Uptime
                </Text>
                <Text as="dd" weight="medium">
                  {formatUptime(status.uptime_seconds)}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Devices
                </Text>
                <Text as="dd" weight="medium">
                  {status.device_count}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Tunnels
                </Text>
                <Text as="dd" weight="medium">
                  {status.tunnel_count}
                </Text>
              </div>
              <div>
                <Text
                  as="dt"
                  size="xs"
                  className="uppercase tracking-wide text-ink-3"
                >
                  Database size
                </Text>
                <Text as="dd" weight="medium">
                  {formatBytes(status.db_size_bytes)}
                </Text>
              </div>
            </Text>
          ) : (
            <Text as="p" size="sm" className="text-ink-3">
              Unable to connect to daemon.
            </Text>
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
        onToggleAutoUpdate={(enabled) =>
          saveConfig.mutate({ auto_update_enabled: enabled })
        }
        onChangeChannel={(channel) => saveConfig.mutate({ channel })}
      />

      <Card>
        <CardHeader>
          <CardTitle>Account</CardTitle>
        </CardHeader>
        <CardContent>
          <Text as="p" size="sm" className="text-ink-3">
            Account management will be available in a future release.
          </Text>
        </CardContent>
      </Card>
    </div>
  );
}
