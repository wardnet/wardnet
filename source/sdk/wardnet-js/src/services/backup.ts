import { WardnetApiError, type WardnetClient } from "../client.js";
import type { ApiError } from "../types/api.js";
import type {
  ApplyImportRequest,
  ApplyImportResponse,
  BackupStatusResponse,
  ExportBackupRequest,
  ListSnapshotsResponse,
  RestorePreviewResponse,
} from "../types/backup.js";

/**
 * Backup / restore service — export an encrypted bundle, preview an
 * import, commit it, and list retained `.bak-<ts>` snapshots.
 *
 * Admin-only on the daemon side; the SDK doesn't enforce auth, it
 * just plumbs the request through the shared `WardnetClient`
 * (credentials include cookies by default).
 *
 * Two methods don't fit the JSON-in / JSON-out shape of
 * `WardnetClient.request`, so they call `WardnetClient.authorizedFetch`
 * directly instead. That still routes header construction through the
 * client, so a subclass's `buildHeaders` override (e.g. bearer auth) is
 * applied to these endpoints too:
 *
 * - `export` returns `application/octet-stream` (the encrypted
 *   bundle bytes). Using `Blob` keeps the SDK free of a Node-vs-browser
 *   split — browser code can `URL.createObjectURL(blob)` for a save
 *   dialog, Node code can `Buffer.from(await blob.arrayBuffer())`.
 * - `previewImport` posts `multipart/form-data` with two fields
 *   (`bundle` + `passphrase`). The fetch call must NOT set
 *   `Content-Type` so the platform can inject the boundary.
 */
export class BackupService {
  constructor(private readonly client: WardnetClient) {}

  /** Current backup subsystem status (admin only). */
  async status(): Promise<BackupStatusResponse> {
    return this.client.request<BackupStatusResponse>("/backup/status");
  }

  /**
   * Produce an encrypted bundle and return the raw bytes as a Blob.
   *
   * The `passphrase` must be at least 12 characters (enforced
   * server-side; the request fails with `400 Bad Request` otherwise).
   * The Blob's MIME type is `application/octet-stream`.
   */
  async export(body: ExportBackupRequest): Promise<Blob> {
    const res = await this.client.authorizedFetch("/backup/export", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      throw await buildError(res);
    }
    return res.blob();
  }

  /**
   * Decrypt a bundle server-side, validate compatibility, and return
   * a preview token the caller can pass to `applyImport`.
   *
   * `bundle` accepts any value `FormData.append` takes — a `Blob`
   * (typical browser path from an `<input type="file">`) or a
   * `Uint8Array` wrapped in a `Blob` (typical Node path).
   */
  async previewImport(bundle: Blob, passphrase: string): Promise<RestorePreviewResponse> {
    const form = new FormData();
    form.append("bundle", bundle, "bundle.wardnet.age");
    form.append("passphrase", passphrase);

    const res = await this.client.authorizedFetch("/backup/import/preview", {
      method: "POST",
      // NB: no Content-Type header — the platform sets it with the
      // multipart boundary when the body is a FormData.
      body: form,
    });
    if (!res.ok) {
      throw await buildError(res);
    }
    return (await res.json()) as RestorePreviewResponse;
  }

  /**
   * Commit a previously-previewed restore. The preview token must
   * have come from a `previewImport` call within the last 5 minutes;
   * otherwise the server responds with `400 Bad Request`.
   */
  async applyImport(body: ApplyImportRequest): Promise<ApplyImportResponse> {
    return this.client.request<ApplyImportResponse>("/backup/import/apply", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Retained `.bak-<ts>` snapshots from prior restores (admin only). */
  async listSnapshots(): Promise<ListSnapshotsResponse> {
    return this.client.request<ListSnapshotsResponse>("/backup/snapshots");
  }
}

/**
 * Turn a non-OK `Response` into a `WardnetApiError`. Mirrors the
 * error-parsing the main `WardnetClient.request` does so callers see
 * the same exception shape regardless of which method failed.
 */
async function buildError(res: Response): Promise<WardnetApiError> {
  const requestId = res.headers.get("X-Request-Id") ?? undefined;
  const body = (await res.json().catch(() => ({ error: res.statusText }))) as ApiError;
  return new WardnetApiError(res.status, res.statusText, body, requestId);
}
