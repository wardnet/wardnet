import { useEffect, useRef, useState } from "react";
import type { RestorePreviewResponse } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import { Card, CardContent, CardHeader, CardTitle } from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
import {
  Modal,
  ModalBody,
  ModalContent,
  ModalDescription,
  ModalFooter,
  ModalHeader,
  ModalTitle,
} from "@wardnet/forge-web/modal";
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
          Export a single encrypted bundle of the database, operator config, and WireGuard keys. Use
          it to restore a fresh install, recover from an SD-card failure, or roll back a risky
          configuration change. Bundles are encrypted with age in passphrase mode.
        </p>

        <div role="alert" className="flex gap-2 rounded-md bg-warn p-3 text-sm text-warn-soft-ink">
          <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
          <span>Keep the passphrase somewhere safe. We can&apos;t recover it for you.</span>
        </div>

        <div className="flex justify-end">
          <Button variant="outline" onClick={() => setExportOpen(true)} disabled={isExporting}>
            <DownloadIcon />
            {isExporting ? "Exporting…" : "Download backup"}
          </Button>
        </div>

        {/* Danger zone — Restore overwrites state and is sequestered into a
            subtle inset block so it never sits next to the safe Download CTA
            (WEBUI-DESIGN-GUIDELINES §4.5). */}
        <div className="flex items-start justify-between gap-4 rounded-md bg-sunken p-4">
          <div className="min-w-0">
            <p className="text-sm font-medium">Restore from backup</p>
            <p className="mt-0.5 text-xs text-muted-foreground">
              Overwrites your current database, config, and keys. Cannot be undone.
            </p>
          </div>
          <Button
            variant="destructive"
            onClick={() => setRestoreOpen(true)}
            disabled={isPreviewing || isApplying}
          >
            <UploadIcon />
            Restore…
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
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <ModalContent>
        <ModalHeader>
          <ModalTitle>Download an encrypted backup</ModalTitle>
        </ModalHeader>
        <ModalBody>
          <ModalDescription>
            Choose a passphrase of at least {MIN_PASSPHRASE_LEN} characters. The passphrase is
            required to restore this bundle; we can&apos;t recover it if you lose it.
          </ModalDescription>

          <div className="flex flex-col gap-3">
            <Field
              label="Passphrase"
              htmlFor="backup-passphrase"
              help={
                tooShort ? (
                  <span className="text-danger">
                    At least {MIN_PASSPHRASE_LEN} characters required.
                  </span>
                ) : undefined
              }
            >
              <Input
                id="backup-passphrase"
                type="password"
                autoComplete="new-password"
                value={passphrase}
                onChange={(e) => setPassphrase(e.target.value)}
              />
            </Field>
            <Field
              label="Confirm passphrase"
              htmlFor="backup-passphrase-confirm"
              help={
                mismatch ? (
                  <span className="text-danger">Passphrases do not match.</span>
                ) : undefined
              }
            >
              <Input
                id="backup-passphrase-confirm"
                type="password"
                autoComplete="new-password"
                value={confirm}
                onChange={(e) => setConfirm(e.target.value)}
              />
            </Field>
          </div>
        </ModalBody>

        <ModalFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={isExporting}>
            Cancel
          </Button>
          <Button onClick={() => onSubmit(passphrase)} disabled={!canSubmit}>
            {isExporting ? "Exporting…" : "Download"}
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
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

  // Dialog instances are kept mounted even when closed, so we explicitly
  // reset local state on every transition to open — a failed preview in
  // a previous session would otherwise leave the file + passphrase
  // pre-filled the next time the operator clicks "Restore". The parent
  // clears preview state via `onDismissPreview` on close; this takes
  // care of the local inputs. Tracked via a ref so the lint rule against
  // setState-in-effect doesn't fire on every render.
  const wasOpenRef = useRef(open);
  useEffect(() => {
    if (open && !wasOpenRef.current) {
      setBundle(null);
      setPassphrase("");
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
    wasOpenRef.current = open;
  }, [open]);

  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next) reset();
        onOpenChange(next);
      }}
    >
      <ModalContent className="flex max-h-[90vh] max-w-lg flex-col">
        <ModalHeader>
          <ModalTitle>Restore from a backup</ModalTitle>
        </ModalHeader>
        <ModalBody className="min-h-0 flex-1 overflow-y-auto">
          <ModalDescription>
            {preview
              ? "Review what will be replaced, then confirm the restore."
              : "Pick a .wardnet.age file and enter its passphrase."}
          </ModalDescription>

          {!preview ? (
            <div className="flex flex-col gap-3">
              <Field label="Bundle file" htmlFor="backup-bundle">
                <input
                  id="backup-bundle"
                  ref={fileInputRef}
                  type="file"
                  accept=".age,.wardnet,.wardnet.age"
                  onChange={(e) => {
                    const file = e.target.files?.[0] ?? null;
                    setBundle(file);
                  }}
                  className="block w-full text-sm file:mr-2 file:rounded-md file:border file:border-line file:bg-sunken file:px-3 file:py-1.5 file:text-sm file:font-medium"
                />
              </Field>
              <Field label="Passphrase" htmlFor="restore-passphrase">
                <Input
                  id="restore-passphrase"
                  type="password"
                  // `off` + `new-password` together suppress every popular
                  // browser's autofill for this field — this is a bundle
                  // passphrase, not a login credential, and we don't want
                  // it remembered or pre-filled from the site's saved
                  // passwords.
                  autoComplete="off"
                  data-lpignore="true"
                  data-1p-ignore="true"
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                />
              </Field>
            </div>
          ) : (
            <RestorePreviewDetails preview={preview} />
          )}
        </ModalBody>

        <ModalFooter>
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
              {isApplying ? "Restoring…" : "Restore"}
            </Button>
          )}
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

function RestorePreviewDetails({ preview }: { preview: RestorePreviewResponse }) {
  return (
    <div className="min-h-0 flex-1 space-y-3 overflow-y-auto text-sm">
      {!preview.compatible && (
        <div className="flex gap-2 rounded-md border border-danger/50 bg-danger/10 p-3 text-danger">
          <AlertTriangleIcon className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="min-w-0">
            <div className="font-medium">Bundle incompatible</div>
            <div className="break-words text-xs">
              {preview.incompatibility_reason ?? "Unknown reason"}
            </div>
          </div>
        </div>
      )}
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <dt className="text-muted-foreground">From version</dt>
        <dd className="break-all font-mono">{preview.manifest.wardnet_version}</dd>
        <dt className="text-muted-foreground">Host ID</dt>
        <dd className="break-all font-mono">{preview.manifest.host_id}</dd>
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
            <li key={f} className="break-all">
              {f}
            </li>
          ))}
        </ul>
      </div>
      <p className="text-xs text-muted-foreground">
        A{" "}
        <code className="rounded bg-sunken px-1 py-0.5 font-mono text-[0.7rem]">
          .bak-&lt;timestamp&gt;
        </code>{" "}
        sibling is kept for every replaced file and retained for 24&nbsp;hours.
      </p>
    </div>
  );
}
