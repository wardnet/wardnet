import { useEffect, useRef, useState } from "react";
import type { RestorePreviewResponse } from "@wardnet/js";
import { Button } from "@wardnet/forge-web/button";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/forge-web/card";
import { Field } from "@wardnet/forge-web/field";
import { Input } from "@wardnet/forge-web/input";
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

type Mode = "view" | "export" | "restore";

/**
 * Pure-presentation card with the two admin actions (Download, Restore).
 * All data + callbacks flow from props; the Settings page owns the
 * TanStack Query state via [`useBackup`] hooks.
 *
 * Follows the edit-mode card protocol from DhcpConfigCard /
 * DeviceSettingsCard: the card swaps body + footer between view, export
 * and restore modes rather than spawning modal dialogs.
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
  const [mode, setMode] = useState<Mode>("view");

  // Export-mode local state.
  const [exportPassphrase, setExportPassphrase] = useState("");
  const [exportConfirm, setExportConfirm] = useState("");
  const tooShort = exportPassphrase.length > 0 && exportPassphrase.length < MIN_PASSPHRASE_LEN;
  const mismatch = exportConfirm.length > 0 && exportConfirm !== exportPassphrase;
  const canExport =
    exportPassphrase.length >= MIN_PASSPHRASE_LEN &&
    exportPassphrase === exportConfirm &&
    !isExporting;

  // Restore-mode local state.
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [bundle, setBundle] = useState<File | null>(null);
  const [restorePassphrase, setRestorePassphrase] = useState("");
  const canPreview =
    bundle !== null && restorePassphrase.length >= MIN_PASSPHRASE_LEN && !isPreviewing;

  // One-shot mount-style effect: clear file/passphrase whenever we (re)enter
  // restore mode, so a stale selection from a previous attempt does not
  // linger. Tracked via a ref so the lint rule against setState-in-effect
  // doesn't fire on every render.
  const mountedRef = useRef(false);
  useEffect(() => {
    if (mode === "restore" && !mountedRef.current) {
      setBundle(null);
      setRestorePassphrase("");
      if (fileInputRef.current) fileInputRef.current.value = "";
      mountedRef.current = true;
    } else if (mode !== "restore") {
      mountedRef.current = false;
    }
  }, [mode]);

  function enterExport() {
    setExportPassphrase("");
    setExportConfirm("");
    setMode("export");
  }

  function enterRestore() {
    setMode("restore");
  }

  function exitToView() {
    setMode("view");
    onDismissPreview();
  }

  function handleExportSubmit() {
    onExport(exportPassphrase);
    setMode("view");
    setExportPassphrase("");
    setExportConfirm("");
  }

  function handleApply() {
    if (!preview) return;
    onApply(preview.preview_token);
    setMode("view");
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Backup &amp; restore</CardTitle>
        {mode === "view" && (
          <CardAction className="flex gap-2">
            <Button variant="outline" onClick={enterExport} disabled={isExporting}>
              <DownloadIcon />
              {isExporting ? "Exporting…" : "Download backup"}
            </Button>
            <Button
              variant="destructive"
              onClick={enterRestore}
              disabled={isPreviewing || isApplying}
            >
              <UploadIcon />
              Restore…
            </Button>
          </CardAction>
        )}
      </CardHeader>

      {mode === "view" && (
        <CardContent className="space-y-4">
          <p className="text-sm text-ink-3">
            Export a single encrypted bundle of the database, operator config, and WireGuard keys.
            Use it to restore a fresh install, recover from an SD-card failure, or roll back a risky
            configuration change. Bundles are encrypted with age in passphrase mode.
          </p>

          <div
            role="alert"
            className="flex gap-2 rounded-md bg-warn p-3 text-sm text-warn-soft-ink"
          >
            <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
            <span>Keep the passphrase somewhere safe. We can&apos;t recover it for you.</span>
          </div>

          {/* Danger zone — Restore overwrites state, sequestered into a
              subtle inset block so it never sits next to the safe Download
              CTA (WEBUI-DESIGN-GUIDELINES §4.5). */}
          <div className="rounded-md bg-sunken p-4">
            <p className="text-sm font-medium">Restore from backup</p>
            <p className="mt-0.5 text-xs text-ink-3">
              Overwrites your current database, config, and keys. Cannot be undone.
            </p>
          </div>
        </CardContent>
      )}

      {mode === "export" && (
        <>
          <CardContent className="space-y-4">
            <p className="text-sm text-ink-3">
              Choose a passphrase of at least {MIN_PASSPHRASE_LEN} characters. The passphrase is
              required to restore this bundle; we can&apos;t recover it if you lose it.
            </p>

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
                  value={exportPassphrase}
                  onChange={(e) => setExportPassphrase(e.target.value)}
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
                  value={exportConfirm}
                  onChange={(e) => setExportConfirm(e.target.value)}
                />
              </Field>
            </div>
          </CardContent>
          <CardFooter className="justify-end gap-2">
            <Button variant="ghost" onClick={exitToView} disabled={isExporting}>
              Cancel
            </Button>
            <Button onClick={handleExportSubmit} disabled={!canExport}>
              {isExporting ? "Exporting…" : "Download"}
            </Button>
          </CardFooter>
        </>
      )}

      {mode === "restore" && (
        <>
          <CardContent className="space-y-4">
            <p className="text-sm text-ink-3">
              {preview
                ? "Review what will be replaced, then confirm the restore."
                : "Pick a .wardnet.age file and enter its passphrase."}
            </p>

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
                    value={restorePassphrase}
                    onChange={(e) => setRestorePassphrase(e.target.value)}
                  />
                </Field>
              </div>
            ) : (
              <RestorePreviewDetails preview={preview} />
            )}
          </CardContent>
          <CardFooter className="justify-end gap-2">
            <Button variant="ghost" onClick={exitToView} disabled={isApplying}>
              Cancel
            </Button>
            {!preview ? (
              <Button
                onClick={() => {
                  if (bundle) onPreview({ bundle, passphrase: restorePassphrase });
                }}
                disabled={!canPreview}
              >
                {isPreviewing ? "Decrypting…" : "Preview"}
              </Button>
            ) : (
              <Button
                variant="destructive"
                onClick={handleApply}
                disabled={!preview.compatible || isApplying}
              >
                {isApplying ? "Restoring…" : "Restore"}
              </Button>
            )}
          </CardFooter>
        </>
      )}
    </Card>
  );
}

function RestorePreviewDetails({ preview }: { preview: RestorePreviewResponse }) {
  return (
    <div className="space-y-3 text-sm">
      {!preview.compatible && (
        <div className="flex gap-2 rounded-md bg-danger-soft p-3 text-danger-soft-ink">
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
        <dt className="text-ink-3">From version</dt>
        <dd className="break-all font-mono">{preview.manifest.wardnet_version}</dd>
        <dt className="text-ink-3">Host ID</dt>
        <dd className="break-all font-mono">{preview.manifest.host_id}</dd>
        <dt className="text-ink-3">Created</dt>
        <dd>{new Date(preview.manifest.created_at).toLocaleString()}</dd>
        <dt className="text-ink-3">Schema version</dt>
        <dd>{preview.manifest.schema_version}</dd>
        <dt className="text-ink-3">WireGuard keys</dt>
        <dd>{preview.manifest.key_count}</dd>
      </dl>
      <div>
        <div className="mb-1 text-ink-3">Will replace:</div>
        <ul className="list-inside list-disc space-y-0.5 font-mono text-xs">
          {preview.files_to_replace.map((f) => (
            <li key={f} className="break-all">
              {f}
            </li>
          ))}
        </ul>
      </div>
      <p className="text-xs text-ink-3">
        A{" "}
        <code className="rounded bg-sunken px-1 py-0.5 font-mono text-[0.7rem]">
          .bak-&lt;timestamp&gt;
        </code>{" "}
        sibling is kept for every replaced file and retained for 24&nbsp;hours.
      </p>
    </div>
  );
}
