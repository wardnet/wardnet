import { useState } from "react";
import type { RestorePreviewResponse } from "@wardnet/js";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { PageHeader } from "@/components/compound/PageHeader";
import { BackupCard } from "@/components/features/BackupCard";
import { UpdateCard } from "@/components/features/UpdateCard";
import { useApplyImport, useExportBackup, usePreviewImport } from "@/hooks/useBackup";
import { useSystemStatus } from "@/hooks/useSystemStatus";
import {
  useCheckForUpdates,
  useInstallUpdate,
  useRollbackUpdate,
  useUpdateConfig,
  useUpdateStatus,
} from "@/hooks/useUpdate";
import { formatBytes, formatUptime } from "@/lib/utils";

/** Settings page for system configuration (admin only). */
export default function Settings() {
  const { data: status, isLoading } = useSystemStatus();
  const { data: updateStatus, isLoading: updateLoading } = useUpdateStatus();
  const check = useCheckForUpdates();
  const install = useInstallUpdate();
  const rollback = useRollbackUpdate();
  const saveConfig = useUpdateConfig();

  // Backup flow — preview response is held in page-local state so the
  // BackupCard can render it between the two steps of the restore
  // wizard. `onDismissPreview` clears it when the operator cancels.
  const [preview, setPreview] = useState<RestorePreviewResponse | null>(null);
  const exportBackup = useExportBackup();
  const previewImport = usePreviewImport();
  const applyImport = useApplyImport();

  return (
    <>
      <PageHeader title="Settings" />
      <div className="flex flex-col gap-4">
        <Card>
          <CardHeader>
            <CardTitle>System Information</CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <p className="text-sm text-muted-foreground">Loading...</p>
            ) : status ? (
              <dl className="grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:grid-cols-3">
                <div>
                  <dt className="text-muted-foreground">Version</dt>
                  <dd className="font-medium">{status.version}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Uptime</dt>
                  <dd className="font-medium">{formatUptime(status.uptime_seconds)}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Devices</dt>
                  <dd className="font-medium">{status.device_count}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Tunnels</dt>
                  <dd className="font-medium">{status.tunnel_count}</dd>
                </div>
                <div>
                  <dt className="text-muted-foreground">Database Size</dt>
                  <dd className="font-medium">{formatBytes(status.db_size_bytes)}</dd>
                </div>
              </dl>
            ) : (
              <p className="text-sm text-muted-foreground">Unable to connect to daemon.</p>
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
              { onSuccess: () => setPreview(null) },
            )
          }
          onDismissPreview={() => setPreview(null)}
        />

        <Card>
          <CardHeader>
            <CardTitle>Account</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm text-muted-foreground">
              Account management will be available in a future release.
            </p>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
