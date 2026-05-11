import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  WardnetApiError,
  type ApplyImportRequest,
  type ApplyImportResponse,
  type BackupStatusResponse,
  type ExportBackupRequest,
  type ListSnapshotsResponse,
  type RestorePreviewResponse,
} from "@wardnet/js";
import { backupService } from "@/lib/sdk";

/** Extract the most user-friendly message we can from an API error. */
function errorMessage(err: unknown, fallback: string): string {
  if (err instanceof WardnetApiError) {
    return err.body.detail ?? err.body.error ?? fallback;
  }
  if (err instanceof Error && err.message) {
    return err.message;
  }
  return fallback;
}

const STATUS_KEY = ["backup", "status"] as const;
const SNAPSHOTS_KEY = ["backup", "snapshots"] as const;

/**
 * Poll the backup subsystem status. The page shows a banner while an
 * import is in flight, so a fairly tight interval keeps the UI honest
 * without being chatty.
 */
export function useBackupStatus() {
  return useQuery<BackupStatusResponse>({
    queryKey: STATUS_KEY,
    queryFn: () => backupService.status(),
    refetchInterval: 10_000,
  });
}

export function useBackupSnapshots() {
  return useQuery<ListSnapshotsResponse>({
    queryKey: SNAPSHOTS_KEY,
    queryFn: () => backupService.listSnapshots(),
  });
}

/**
 * Export a bundle. Success triggers a browser download of the
 * encrypted `.wardnet.age` blob with a sensible default filename.
 * The mutation resolves to the Blob so callers that want to keep the
 * bytes in memory (e.g. for upload to an off-box destination) can.
 */
export function useExportBackup() {
  return useMutation<Blob, unknown, ExportBackupRequest>({
    mutationFn: (body) => backupService.export(body),
    onSuccess: (blob) => {
      const ts = new Date()
        .toISOString()
        .replace(/[-:.TZ]/g, "")
        .slice(0, 15);
      const filename = `wardnet-${ts}Z.wardnet.age`;
      triggerBrowserDownload(blob, filename);
      toast.success("Backup downloaded");
    },
    onError: (err) => toast.error(errorMessage(err, "Backup export failed")),
  });
}

/** Preview an import — decrypts server-side, returns manifest + token. */
export function usePreviewImport() {
  return useMutation<RestorePreviewResponse, unknown, { bundle: Blob; passphrase: string }>({
    mutationFn: ({ bundle, passphrase }) => backupService.previewImport(bundle, passphrase),
    onError: (err) => toast.error(errorMessage(err, "Bundle could not be decrypted")),
  });
}

/**
 * Commit a previously-previewed import. On success the daemon has
 * swapped the live files and set `backup_restart_pending=true`; the
 * caller is responsible for surfacing the "restart required" banner.
 */
export function useApplyImport() {
  const qc = useQueryClient();
  return useMutation<ApplyImportResponse, unknown, ApplyImportRequest>({
    mutationFn: (body) => backupService.applyImport(body),
    onSuccess: () => {
      toast.success("Backup restored — restart the daemon to complete");
      qc.invalidateQueries({ queryKey: STATUS_KEY });
      qc.invalidateQueries({ queryKey: SNAPSHOTS_KEY });
    },
    onError: (err) => toast.error(errorMessage(err, "Restore failed")),
  });
}

/**
 * Build an `<a download>` link, click it, then release the object
 * URL. The Blob never touches disk outside the browser's download
 * flow — in particular, plaintext secrets inside the archive are
 * encrypted before they ever leave the daemon.
 */
function triggerBrowserDownload(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}
