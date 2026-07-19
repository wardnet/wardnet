import { useState } from "react";
import { useNavigate } from "react-router";
import type { RestorePreviewResponse } from "@wardnet/js";
import { PageHeader } from "@/components/compound/PageHeader";
import { BackupCard } from "@/components/features/BackupCard";
import { RestartProgressDialog } from "@/components/features/RestartProgressDialog";
import {
  useApplyImport,
  useExportBackup,
  usePreviewImport,
} from "@wardnet/web";
import { useRestart } from "@wardnet/web";
import { useAuthStore } from "@wardnet/web";

/** Backups page (admin only) — export and restore the Wardnet
 *  bundle. The post-restore daemon restart is part of this flow, so
 *  `useRestart` + `RestartProgressDialog` live alongside the card. */
export default function Backups() {
  const navigate = useNavigate();
  const logout = useAuthStore((s) => s.logout);

  // Backup flow — preview response is held in page-local state so the
  // BackupCard can render it between the two steps of the restore
  // wizard. `onDismissPreview` clears it when the operator cancels.
  const [preview, setPreview] = useState<RestorePreviewResponse | null>(null);
  const exportBackup = useExportBackup();
  const previewImport = usePreviewImport();
  const applyImport = useApplyImport();

  // Restore writes new files; the daemon must restart for the live
  // pool to pick them up. We reuse the standard restart dialog when
  // an apply succeeds.
  const restart = useRestart();

  const onRestartReady = () => {
    restart.reset();
  };
  const onRestartSignIn = () => {
    // Session cookie was invalidated by the restart (e.g. in-memory
    // session store). Clear local admin flag and route to login.
    void logout();
    restart.reset();
    void navigate("/login");
  };

  return (
    <>
      <div className="col gap-20">
        <PageHeader
          title="Backups"
          description="Export an encrypted snapshot of your configuration, or restore from a previous bundle."
        />

        <BackupCard
          isExporting={exportBackup.isPending}
          isPreviewing={previewImport.isPending}
          isApplying={applyImport.isPending}
          preview={preview}
          exportError={exportBackup.error}
          previewError={previewImport.error}
          applyError={applyImport.error}
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
      </div>

      {/* Restart dialog used by the post-restore flow. */}
      <RestartProgressDialog
        open={restart.isOpen}
        phase={restart.phase}
        startedAt={restart.startedAt}
        errorMessage={restart.errorMessage}
        onDismiss={onRestartReady}
        onSignIn={onRestartSignIn}
      />
    </>
  );
}
