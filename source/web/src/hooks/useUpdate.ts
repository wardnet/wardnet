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

/** Poll the update status at ~15 s so banners reflect new releases quickly. */
export function useUpdateStatus() {
  return useQuery<UpdateStatusResponse>({
    queryKey: STATUS_KEY,
    queryFn: () => updateService.status(),
    refetchInterval: 15_000,
  });
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
      toast.success("Rollback staged — daemon will restart", {
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
