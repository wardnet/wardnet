import { useRef, useState } from "react";
import type { RestorePreviewResponse } from "@wardnet/js";
import { Button } from "@/components/core/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/core/ui/dialog";
import { DownloadIcon, UploadIcon, AlertTriangleIcon } from "lucide-react";

/** Server-enforced minimum, mirrors `wardnet_common::backup::MIN_PASSPHRASE_LEN`. */
const MIN_PASSPHRASE_LEN = 12;

interface Props {
  isExporting: boolean;
  isPreviewing: boolean;
  isApplying: boolean;
  preview: RestorePreviewResponse | null;
  onExport: (passphrase: string) => void;
  onPreview: (args: { bundle: Blob; passphrase: string }) => void;
  onApply: (previewToken: string) => void;
  onDismissPreview: () => void;
}

/**
 * Pure-presentation card with the two admin actions (Download, Restore)
 * and their confirmation dialogs. All data + callbacks flow from
 * props; the Settings page owns the TanStack Query state via
 * [`useBackup`] hooks.
 *
 * The restore flow is deliberately two-step:
 *
 * 1. Pick a file + enter passphrase → `onPreview` decrypts server-side
 *    and returns a `RestorePreviewResponse` with a 5-min preview token
 *    plus a summary of what will be replaced.
 * 2. Show the preview + an explicit "Apply" confirmation →
 *    `onApply(previewToken)` commits the swap. A daemon restart is
 *    required afterwards for the live DB pool to pick up the new file.
 */
export function BackupCard({
  isExporting,
  isPreviewing,
  isApplying,
  preview,
  onExport,
  onPreview,
  onApply,
  onDismissPreview,
}: Props) {
  const [exportOpen, setExportOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Backup &amp; restore</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">
          Export a single encrypted bundle of the database, operator config, and WireGuard keys — or
          restore one on a fresh install. Bundles are encrypted with age (passphrase mode); keep the
          passphrase somewhere safe.
        </p>

        <div className="flex gap-2">
          <Button onClick={() => setExportOpen(true)} disabled={isExporting}>
            <DownloadIcon className="mr-2 h-4 w-4" />
            {isExporting ? "Exporting…" : "Download backup"}
          </Button>
          <Button
            variant="outline"
            onClick={() => setRestoreOpen(true)}
            disabled={isPreviewing || isApplying}
          >
            <UploadIcon className="mr-2 h-4 w-4" />
            Restore from backup
          </Button>
        </div>
      </CardContent>

      <ExportDialog
        open={exportOpen}
        onOpenChange={setExportOpen}
        isExporting={isExporting}
        onSubmit={(passphrase) => {
          onExport(passphrase);
          setExportOpen(false);
        }}
      />

      <RestoreDialog
        open={restoreOpen}
        onOpenChange={(next) => {
          setRestoreOpen(next);
          if (!next) onDismissPreview();
        }}
        isPreviewing={isPreviewing}
        isApplying={isApplying}
        preview={preview}
        onPreview={onPreview}
        onApply={(token) => {
          onApply(token);
          setRestoreOpen(false);
        }}
        onDismissPreview={onDismissPreview}
      />
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Export dialog — passphrase prompt
// ---------------------------------------------------------------------------

function ExportDialog({
  open,
  onOpenChange,
  isExporting,
  onSubmit,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isExporting: boolean;
  onSubmit: (passphrase: string) => void;
}) {
  const [passphrase, setPassphrase] = useState("");
  const [confirm, setConfirm] = useState("");
  const tooShort = passphrase.length > 0 && passphrase.length < MIN_PASSPHRASE_LEN;
  const mismatch = confirm.length > 0 && confirm !== passphrase;
  const canSubmit =
    passphrase.length >= MIN_PASSPHRASE_LEN && passphrase === confirm && !isExporting;

  const reset = () => {
    setPassphrase("");
    setConfirm("");
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Download an encrypted backup</DialogTitle>
          <DialogDescription>
            Choose a passphrase — at least {MIN_PASSPHRASE_LEN} characters. The passphrase is
            required to restore this bundle; we can&apos;t recover it if you lose it.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="space-y-1">
            <Label htmlFor="backup-passphrase">Passphrase</Label>
            <Input
              id="backup-passphrase"
              type="password"
              autoComplete="new-password"
              value={passphrase}
              onChange={(e) => setPassphrase(e.target.value)}
            />
            {tooShort && (
              <p className="text-xs text-destructive">
                At least {MIN_PASSPHRASE_LEN} characters required.
              </p>
            )}
          </div>
          <div className="space-y-1">
            <Label htmlFor="backup-passphrase-confirm">Confirm passphrase</Label>
            <Input
              id="backup-passphrase-confirm"
              type="password"
              autoComplete="new-password"
              value={confirm}
              onChange={(e) => setConfirm(e.target.value)}
            />
            {mismatch && <p className="text-xs text-destructive">Passphrases do not match.</p>}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isExporting}>
            Cancel
          </Button>
          <Button onClick={() => onSubmit(passphrase)} disabled={!canSubmit}>
            {isExporting ? "Exporting…" : "Download"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ---------------------------------------------------------------------------
// Restore dialog — file picker + passphrase, preview, confirm
// ---------------------------------------------------------------------------

function RestoreDialog({
  open,
  onOpenChange,
  isPreviewing,
  isApplying,
  preview,
  onPreview,
  onApply,
  onDismissPreview,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isPreviewing: boolean;
  isApplying: boolean;
  preview: RestorePreviewResponse | null;
  onPreview: (args: { bundle: Blob; passphrase: string }) => void;
  onApply: (previewToken: string) => void;
  onDismissPreview: () => void;
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [bundle, setBundle] = useState<File | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const canPreview = bundle !== null && passphrase.length >= MIN_PASSPHRASE_LEN && !isPreviewing;

  const reset = () => {
    setBundle(null);
    setPassphrase("");
    if (fileInputRef.current) fileInputRef.current.value = "";
    onDismissPreview();
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>Restore from a backup</DialogTitle>
          <DialogDescription>
            {preview
              ? "Review what will be replaced, then confirm the restore."
              : "Pick a .wardnet.age file and enter its passphrase."}
          </DialogDescription>
        </DialogHeader>

        {!preview ? (
          <div className="space-y-3">
            <div className="space-y-1">
              <Label htmlFor="backup-bundle">Bundle file</Label>
              <input
                id="backup-bundle"
                ref={fileInputRef}
                type="file"
                accept=".age,.wardnet,.wardnet.age"
                onChange={(e) => {
                  const file = e.target.files?.[0] ?? null;
                  setBundle(file);
                }}
                className="block w-full text-sm file:mr-2 file:rounded-md file:border file:border-input file:bg-muted file:px-3 file:py-1.5 file:text-sm file:font-medium"
              />
            </div>
            <div className="space-y-1">
              <Label htmlFor="restore-passphrase">Passphrase</Label>
              <Input
                id="restore-passphrase"
                type="password"
                autoComplete="current-password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </div>
          </div>
        ) : (
          <RestorePreviewDetails preview={preview} />
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isApplying}>
            Cancel
          </Button>
          {!preview ? (
            <Button
              onClick={() => {
                if (bundle) onPreview({ bundle, passphrase });
              }}
              disabled={!canPreview}
            >
              {isPreviewing ? "Decrypting…" : "Preview"}
            </Button>
          ) : (
            <Button
              variant="destructive"
              onClick={() => onApply(preview.preview_token)}
              disabled={!preview.compatible || isApplying}
            >
              {isApplying ? "Applying…" : "Apply restore"}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function RestorePreviewDetails({ preview }: { preview: RestorePreviewResponse }) {
  return (
    <div className="space-y-3 text-sm">
      {!preview.compatible && (
        <div className="flex gap-2 rounded-md border border-destructive/50 bg-destructive/10 p-3 text-destructive">
          <AlertTriangleIcon className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <div className="font-medium">Bundle incompatible</div>
            <div className="text-xs">{preview.incompatibility_reason ?? "Unknown reason"}</div>
          </div>
        </div>
      )}
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt className="text-muted-foreground">From version</dt>
        <dd className="font-mono">{preview.manifest.wardnet_version}</dd>
        <dt className="text-muted-foreground">Host ID</dt>
        <dd className="font-mono">{preview.manifest.host_id}</dd>
        <dt className="text-muted-foreground">Created</dt>
        <dd>{new Date(preview.manifest.created_at).toLocaleString()}</dd>
        <dt className="text-muted-foreground">Schema version</dt>
        <dd>{preview.manifest.schema_version}</dd>
        <dt className="text-muted-foreground">WireGuard keys</dt>
        <dd>{preview.manifest.key_count}</dd>
      </dl>
      <div>
        <div className="mb-1 text-muted-foreground">Will replace:</div>
        <ul className="list-inside list-disc space-y-0.5 font-mono text-xs">
          {preview.files_to_replace.map((f) => (
            <li key={f}>{f}</li>
          ))}
        </ul>
      </div>
      <p className="text-xs text-muted-foreground">
        A `.bak-&lt;timestamp&gt;` sibling is kept for every replaced file and retained for
        24&nbsp;hours. Restart the daemon after the restore completes.
      </p>
    </div>
  );
}
