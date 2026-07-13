import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import {
  WardnetApiError,
  type InstallUpdateRequest,
  type UpdateConfigRequest,
  type UpdateHistoryResponse,
  type UpdateStatusResponse,
} from "@wardnet/js";
import { updateService } from "../lib/sdk";

/**
 * Extract the most user-friendly message we can from an API error.
 *
 * For [`WardnetApiError`] the server sets `body.detail` on variants it
 * wants to surface verbatim (`BadRequest`, `Conflict`, `UpstreamUnavailable`, ...).
 * Fall back to `body.error` (the status label) and finally to the generic
 * `fallback`. Non-API errors (network failure, aborted fetch) fall back to
 * the JS `Error.message`.
 */
function errorMessage(err: unknown, fallback: string): string {
  if (err instanceof WardnetApiError) {
    return err.body.detail ?? err.body.error ?? fallback;
  }
  if (err instanceof Error && err.message) {
    return err.message;
  }
  return fallback;
}

const STATUS_KEY = ["update", "status"] as const;
const HISTORY_KEY = ["update", "history"] as const;

const TOAST_APPLIED = "update-applied";
/**
 * Last `applied_at` we announced. Persisted because the announcement has to
 * survive the very page load it is announcing: the daemon restarts under us,
 * so the browser that started the install is typically remounting when the
 * news arrives.
 */
const APPLIED_SEEN_KEY = "wardnet.update.appliedAt";

function readAppliedSeen(): string | null {
  try {
    return window.localStorage.getItem(APPLIED_SEEN_KEY);
  } catch {
    // Private mode / storage disabled — degrade to announcing once per mount
    // rather than breaking the status poll.
    return null;
  }
}

function writeAppliedSeen(value: string): void {
  try {
    window.localStorage.setItem(APPLIED_SEEN_KEY, value);
  } catch {
    // Ignore: worst case the toast repeats on the next mount.
  }
}

/**
 * Announced in this page's lifetime. `useUpdateStatus` has several concurrent
 * consumers (the sidebar banner and the settings card, at least), all sharing
 * one query cache, so they all run the announce effect on the same commit.
 * Relying on the localStorage read-then-write to serialize would be relying on
 * React's effect ordering; this claims the timestamp synchronously instead, so
 * exactly one consumer announces regardless of how the effects interleave.
 */
const announcedThisSession = new Set<string>();

/**
 * Poll the update status at ~15 s so banners reflect new releases quickly.
 *
 * Also announces a completed update. The daemon has no event channel to the
 * browser — it restarts mid-install and this poll is the only thing that comes
 * back — so a successful upgrade is reported by the *status* carrying
 * `applied_version` / `applied_at`, stamped by the daemon's startup reconcile.
 * We announce each `applied_at` exactly once per browser.
 */
export function useUpdateStatus() {
  const query = useQuery<UpdateStatusResponse>({
    queryKey: STATUS_KEY,
    queryFn: () => updateService.status(),
    refetchInterval: 15_000,
  });

  const status = query.data?.status;
  const appliedAt = status?.applied_at ?? null;
  const appliedVersion = status?.applied_version ?? null;

  useEffect(() => {
    if (!appliedAt || !appliedVersion) return;
    // Claim it synchronously before any await/render can interleave.
    if (announcedThisSession.has(appliedAt)) return;
    if (readAppliedSeen() === appliedAt) return;
    announcedThisSession.add(appliedAt);
    writeAppliedSeen(appliedAt);
    toast.success(`Wardnet updated to v${appliedVersion}`, {
      id: TOAST_APPLIED,
    });
  }, [appliedAt, appliedVersion]);

  return query;
}

export function useUpdateHistory(limit = 20) {
  return useQuery<UpdateHistoryResponse>({
    queryKey: [...HISTORY_KEY, limit],
    queryFn: () => updateService.history(limit),
  });
}

// Stable toast IDs per action — rapid back-to-back triggers (e.g.
// switching channel then immediately hitting Check now) collapse
// into the same slot instead of producing a second toast that
// sometimes flashes empty before settling.
const TOAST_CHECK = "update-check";
const TOAST_INSTALL = "update-install";
const TOAST_ROLLBACK = "update-rollback";
const TOAST_CONFIG = "update-config";

export function useCheckForUpdates() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => updateService.check(),
    onSuccess: (data) => {
      qc.setQueryData(STATUS_KEY, data);
      if (data.status.update_available) {
        toast.success(`Update available: v${data.status.latest_version}`, {
          id: TOAST_CHECK,
        });
      } else {
        toast.success("Wardnet is up to date", { id: TOAST_CHECK });
      }
    },
    onError: (err) =>
      toast.error(errorMessage(err, "Update check failed"), {
        id: TOAST_CHECK,
      }),
  });
}

export function useInstallUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: InstallUpdateRequest = {}) =>
      updateService.install(body),
    onSuccess: (data) => {
      toast.success(`Installing v${data.handle.target_version}...`, {
        id: TOAST_INSTALL,
      });
      qc.invalidateQueries({ queryKey: STATUS_KEY });
    },
    onError: (err) =>
      toast.error(errorMessage(err, "Install failed"), { id: TOAST_INSTALL }),
  });
}

export function useRollbackUpdate() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => updateService.rollback(),
    onSuccess: () => {
      toast.success("Rollback staged - daemon will restart", {
        id: TOAST_ROLLBACK,
      });
      qc.invalidateQueries({ queryKey: STATUS_KEY });
    },
    onError: (err) =>
      toast.error(errorMessage(err, "Rollback failed"), { id: TOAST_ROLLBACK }),
  });
}

export function useUpdateConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateConfigRequest) => updateService.updateConfig(body),
    onSuccess: (data) => {
      qc.setQueryData(STATUS_KEY, { status: data.status });
      toast.success("Update settings saved", { id: TOAST_CONFIG });
    },
    onError: (err) =>
      toast.error(errorMessage(err, "Failed to save update settings"), {
        id: TOAST_CONFIG,
      }),
  });
}
