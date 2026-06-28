import { useEffect, useRef, useState } from "react";
import { WardnetApiError, type RestorePreviewResponse } from "@wardnet/js";
import { Button } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import {
  AlertTriangleIcon,
  DownloadIcon,
  Loader2,
  UploadIcon,
} from "lucide-react";
import { ApiErrorAlert } from "@wardnet/web";
import { formatDateTime } from "@wardnet/web";

/** Server-enforced minimum, mirrors `wardnet_common::backup::MIN_PASSPHRASE_LEN`. */
const MIN_PASSPHRASE_LEN = 12;

interface Props {
  isExporting: boolean;
  isPreviewing: boolean;
  isApplying: boolean;
  preview: RestorePreviewResponse | null;
  /** Most recent error from each mutation. When present the matching
   *  form stays open with an inline ApiErrorAlert so the user can
   *  see what failed and retry without losing context. */
  exportError?: unknown;
  previewError?: unknown;
  applyError?: unknown;
  onExport: (passphrase: string) => void;
  onPreview: (args: { bundle: Blob; passphrase: string }) => void;
  onApply: (previewToken: string) => void;
  onDismissPreview: () => void;
}

/** Friendly copy for the most common restore failure (wrong
 *  passphrase / corrupted bundle). The backend surfaces this as a
 *  multipart parse / decrypt error in `WardnetApiError.body.detail`. */
function restoreErrorMessage(err: unknown): string {
  if (err instanceof WardnetApiError) {
    const detail = err.body.detail ?? "";
    if (
      detail.includes("bundle read failed") ||
      detail.toLowerCase().includes("decrypt") ||
      detail.toLowerCase().includes("multipart")
    ) {
      return "Couldn't open the bundle. The passphrase might be wrong, or the file isn't a valid Wardnet backup.";
    }
  }
  return "Couldn't open the bundle.";
}

/**
 * Backup & restore controls — two sibling cards.
 *
 *  - Backup (top): export the current state as an encrypted bundle.
 *  - Restore (bottom, danger-tinted): overwrite current state from
 *    a bundle. Two-step flow — preview, then apply.
 *
 *  Each card stays in its form/preview state while the mutation is
 *  in-flight; the action button shows a Loader2 spinner and inputs
 *  are disabled. We only flip back to the collapsed view after a
 *  successful settlement (tracked via `wasExporting`/`wasApplying`
 *  refs against the corresponding error props).
 */
export function BackupCard({
  isExporting,
  isPreviewing,
  isApplying,
  preview,
  exportError,
  previewError,
  applyError,
  onExport,
  onPreview,
  onApply,
  onDismissPreview,
}: Props) {
  // Two independent expand/collapse states — one per card.
  const [exporting, setExporting] = useState(false);
  const [restoring, setRestoring] = useState(false);

  // Export-mode local state.
  const [exportPassphrase, setExportPassphrase] = useState("");
  const [exportConfirm, setExportConfirm] = useState("");

  // Restore-mode local state.
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [bundle, setBundle] = useState<File | null>(null);
  const [restorePassphrase, setRestorePassphrase] = useState("");

  // After an export settles, collapse the form on success — keep
  // it open with the inline error on failure.
  const wasExporting = useRef(false);
  useEffect(() => {
    if (wasExporting.current && !isExporting && exporting && !exportError) {
      setExporting(false);
      setExportPassphrase("");
      setExportConfirm("");
    }
    wasExporting.current = isExporting;
  }, [isExporting, exporting, exportError]);

  // After an apply settles, collapse the restore form and clear it.
  const wasApplying = useRef(false);
  useEffect(() => {
    if (wasApplying.current && !isApplying && restoring && !applyError) {
      setRestoring(false);
      setBundle(null);
      setRestorePassphrase("");
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
    wasApplying.current = isApplying;
  }, [isApplying, restoring, applyError]);

  function enterExport() {
    setExportPassphrase("");
    setExportConfirm("");
    setExporting(true);
  }
  function exitExport() {
    setExporting(false);
  }

  function enterRestore() {
    setBundle(null);
    setRestorePassphrase("");
    if (fileInputRef.current) fileInputRef.current.value = "";
    onDismissPreview();
    setRestoring(true);
  }
  function exitRestore() {
    setRestoring(false);
    onDismissPreview();
  }

  function handleExportSubmit(values: { passphrase: string; confirm: string }) {
    onExport(values.passphrase);
  }
  function handleRestoreSubmit(values: {
    bundle: File | null;
    passphrase: string;
  }) {
    if (values.bundle)
      onPreview({ bundle: values.bundle, passphrase: values.passphrase });
  }
  function handleApply() {
    if (!preview) return;
    onApply(preview.preview_token);
  }

  return (
    <div className="flex flex-col gap-4">
      {/* ──────────────── Backup card ──────────────── */}
      <Card>
        <CardHeader>
          <CardTitle>Backup</CardTitle>
          {!exporting && (
            <CardAction>
              <Button
                variant="outline"
                onClick={enterExport}
                disabled={isExporting}
                data-testid="backup-export-open"
              >
                <DownloadIcon />
                Download backup
              </Button>
            </CardAction>
          )}
        </CardHeader>

        {!exporting ? (
          <CardContent>
            <Text as="p" size="sm" className="text-ink-3">
              Export an encrypted bundle of the database, operator config, and
              WireGuard keys. Use it to restore a fresh install, recover from
              disk failure, or roll back a risky configuration change.
            </Text>
            <Text
              as="div"
              size="sm"
              className="mt-3 flex gap-2 rounded-md bg-warn p-3 text-warn-soft-ink"
            >
              <AlertTriangleIcon className="mt-0.5 size-4 shrink-0" />
              <span>
                Keep the passphrase somewhere safe. We can&apos;t recover it for
                you.
              </span>
            </Text>
          </CardContent>
        ) : (
          <Form
            values={{ passphrase: exportPassphrase, confirm: exportConfirm }}
            onSubmit={handleExportSubmit}
          >
            <CardContent className="flex flex-col gap-4">
              <Text as="p" size="sm" className="text-ink-3">
                Choose a passphrase of at least {MIN_PASSPHRASE_LEN} characters.
                The passphrase is required to restore this bundle — we
                can&apos;t recover it if you lose it.
              </Text>

              <Field
                label="Passphrase"
                htmlFor="backup-passphrase"
                name="passphrase"
              >
                <Input
                  id="backup-passphrase"
                  data-testid="backup-passphrase"
                  type="password"
                  autoComplete="new-password"
                  value={exportPassphrase}
                  onChange={(e) => setExportPassphrase(e.target.value)}
                  disabled={isExporting}
                />
              </Field>
              <Validator
                name="passphrase"
                rule="required"
                message="Passphrase is required."
              />
              <Validator
                name="passphrase"
                validate={(v) =>
                  typeof v === "string" &&
                  v.length > 0 &&
                  v.length < MIN_PASSPHRASE_LEN
                    ? `At least ${MIN_PASSPHRASE_LEN} characters required.`
                    : null
                }
              />

              <Field
                label="Confirm passphrase"
                htmlFor="backup-passphrase-confirm"
                name="confirm"
              >
                <Input
                  id="backup-passphrase-confirm"
                  data-testid="backup-passphrase-confirm"
                  type="password"
                  autoComplete="new-password"
                  value={exportConfirm}
                  onChange={(e) => setExportConfirm(e.target.value)}
                  disabled={isExporting}
                />
              </Field>
              <Validator
                name="confirm"
                rule="required"
                message="Please confirm the passphrase."
              />
              <Validator
                name="confirm"
                validate={(v) =>
                  v !== exportPassphrase ? "Passphrases do not match." : null
                }
              />

              {exportError != null && (
                <ApiErrorAlert
                  error={exportError}
                  fallback="Couldn't export the backup."
                />
              )}
            </CardContent>
            <CardFooter className="justify-end gap-2">
              <Button
                variant="ghost"
                type="button"
                onClick={exitExport}
                disabled={isExporting}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={isExporting}
                data-testid="backup-export-submit"
              >
                {isExporting ? (
                  <>
                    <Loader2 className="animate-spin" />
                    Exporting…
                  </>
                ) : (
                  "Download"
                )}
              </Button>
            </CardFooter>
          </Form>
        )}
      </Card>

      {/* ──────────────── Restore card ────────────────
          Danger-soft background only in the collapsed "warning"
          state. Once the user clicks Restore… and the form opens we
          revert to the standard card surface so the white input
          fields don't clash with a red wash. */}
      <Card
        style={!restoring ? { background: "var(--danger-soft)" } : undefined}
      >
        <CardHeader>
          <CardTitle>Restore</CardTitle>
          {!restoring && (
            <CardAction>
              <Button
                variant="destructive"
                onClick={enterRestore}
                disabled={isPreviewing || isApplying}
                data-testid="backup-restore-open"
              >
                <UploadIcon />
                Restore…
              </Button>
            </CardAction>
          )}
        </CardHeader>

        {!restoring ? (
          <CardContent>
            <Text as="p" size="sm" className="text-danger-soft-ink">
              Overwrite the current database, config, and WireGuard keys from a
              previously exported bundle. The daemon restarts at the end of a
              restore and the action cannot be undone.
            </Text>
          </CardContent>
        ) : !preview ? (
          <Form
            values={{ bundle, passphrase: restorePassphrase }}
            onSubmit={handleRestoreSubmit}
          >
            <CardContent className="flex flex-col gap-4">
              <Text as="p" size="sm" className="text-ink-3">
                Pick a .wardnet.age file and enter its passphrase.
              </Text>

              <Field label="Bundle file" htmlFor="backup-bundle" name="bundle">
                <input
                  id="backup-bundle"
                  data-testid="backup-bundle"
                  ref={fileInputRef}
                  type="file"
                  accept=".age,.wardnet,.wardnet.age"
                  disabled={isPreviewing}
                  onChange={(e) => setBundle(e.target.files?.[0] ?? null)}
                  className="block w-full text-sm file:mr-2 file:rounded-md file:border file:border-line file:bg-sunken file:px-3 file:py-1.5 file:text-sm file:font-medium"
                />
              </Field>
              <Validator
                name="bundle"
                validate={(v) =>
                  v ? null : "Select a backup bundle to restore."
                }
              />

              <Field
                label="Passphrase"
                htmlFor="restore-passphrase"
                name="passphrase"
              >
                <Input
                  id="restore-passphrase"
                  data-testid="restore-passphrase"
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
                  disabled={isPreviewing}
                />
              </Field>
              <Validator
                name="passphrase"
                rule="required"
                message="Passphrase is required."
              />
              <Validator
                name="passphrase"
                validate={(v) =>
                  typeof v === "string" &&
                  v.length > 0 &&
                  v.length < MIN_PASSPHRASE_LEN
                    ? `At least ${MIN_PASSPHRASE_LEN} characters required.`
                    : null
                }
              />

              {previewError != null && (
                <ApiErrorAlert
                  error={previewError}
                  fallback={restoreErrorMessage(previewError)}
                />
              )}
            </CardContent>
            <CardFooter className="justify-end gap-2">
              <Button
                variant="ghost"
                type="button"
                onClick={exitRestore}
                disabled={isPreviewing}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={isPreviewing}
                data-testid="backup-preview-submit"
              >
                {isPreviewing ? (
                  <>
                    <Loader2 className="animate-spin" />
                    Decrypting…
                  </>
                ) : (
                  "Preview"
                )}
              </Button>
            </CardFooter>
          </Form>
        ) : (
          <>
            <CardContent className="space-y-4">
              <Text as="p" size="sm" className="text-ink-3">
                Review what will be replaced, then confirm the restore.
              </Text>
              <RestorePreviewDetails preview={preview} />
              {applyError != null && (
                <ApiErrorAlert
                  error={applyError}
                  fallback="Couldn't apply the restore."
                />
              )}
            </CardContent>
            <CardFooter className="justify-end gap-2">
              <Button
                variant="ghost"
                onClick={exitRestore}
                disabled={isApplying}
              >
                Cancel
              </Button>
              <Button
                variant="destructive"
                onClick={handleApply}
                disabled={!preview.compatible || isApplying}
                data-testid="backup-apply-submit"
              >
                {isApplying ? (
                  <>
                    <Loader2 className="animate-spin" />
                    Restoring…
                  </>
                ) : (
                  "Restore"
                )}
              </Button>
            </CardFooter>
          </>
        )}
      </Card>
    </div>
  );
}

function RestorePreviewDetails({
  preview,
}: {
  preview: RestorePreviewResponse;
}) {
  return (
    <Text
      as="div"
      size="sm"
      className="space-y-3"
      data-testid="restore-preview"
    >
      {!preview.compatible && (
        <div className="flex gap-2 rounded-md bg-danger-soft p-3 text-danger-soft-ink">
          <AlertTriangleIcon className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="min-w-0">
            <Text as="div" weight="medium">
              Bundle incompatible
            </Text>
            <Text as="div" size="xs" className="break-words">
              {preview.incompatibility_reason ?? "Unknown reason"}
            </Text>
          </div>
        </div>
      )}
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        <Text as="dt" className="text-ink-3">
          From version
        </Text>
        <Text as="dd" className="break-all font-mono">
          {preview.manifest.wardnet_version}
        </Text>
        <Text as="dt" className="text-ink-3">
          Host ID
        </Text>
        <Text as="dd" className="break-all font-mono">
          {preview.manifest.host_id}
        </Text>
        <Text as="dt" className="text-ink-3">
          Created
        </Text>
        <dd>{formatDateTime(preview.manifest.created_at)}</dd>
        <Text as="dt" className="text-ink-3">
          Schema version
        </Text>
        <dd>{preview.manifest.schema_version}</dd>
        <Text as="dt" className="text-ink-3">
          WireGuard keys
        </Text>
        <dd>{preview.manifest.key_count}</dd>
      </dl>
      <div>
        <Text as="div" className="mb-1 text-ink-3">
          Will replace:
        </Text>
        <Text
          as="ul"
          size="xs"
          className="list-inside list-disc space-y-0.5 font-mono"
        >
          {preview.files_to_replace.map((f) => (
            <li key={f} className="break-all">
              {f}
            </li>
          ))}
        </Text>
      </div>
      <Text as="p" size="xs" className="text-ink-3">
        A{" "}
        <code className="rounded bg-sunken px-1 py-0.5 font-mono text-[0.7rem]">
          .bak-&lt;timestamp&gt;
        </code>{" "}
        sibling is kept for every replaced file and retained for 24&nbsp;hours.
      </Text>
    </Text>
  );
}
