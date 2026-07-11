import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WardnetApiError } from "@wardnet/js";
import { createQueryWrapper } from "../test-utils";

const { backupService, toast } = vi.hoisted(() => ({
  backupService: {
    status: vi.fn(),
    listSnapshots: vi.fn(),
    export: vi.fn(),
    previewImport: vi.fn(),
    applyImport: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ backupService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import {
  useBackupStatus,
  useBackupSnapshots,
  useExportBackup,
  usePreviewImport,
  useApplyImport,
} from "../../src/hooks/useBackup";

const wrapper = createQueryWrapper;

describe("useBackup", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Stub the browser download plumbing used by useExportBackup.
    globalThis.URL.createObjectURL = vi.fn(() => "blob:mock");
    globalThis.URL.revokeObjectURL = vi.fn();
  });

  it("polls status and lists snapshots", async () => {
    backupService.status.mockResolvedValue({ importing: false });
    backupService.listSnapshots.mockResolvedValue({ snapshots: [] });
    const { result: s } = renderHook(() => useBackupStatus(), {
      wrapper: wrapper(),
    });
    const { result: l } = renderHook(() => useBackupSnapshots(), {
      wrapper: wrapper(),
    });
    await waitFor(() => expect(s.current.isSuccess).toBe(true));
    await waitFor(() => expect(l.current.isSuccess).toBe(true));
  });

  it("exports a backup and triggers a download", async () => {
    const blob = new Blob(["x"]);
    backupService.export.mockResolvedValue(blob);
    const { result } = renderHook(() => useExportBackup(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ passphrase: "pw" });
    });
    expect(globalThis.URL.createObjectURL).toHaveBeenCalledWith(blob);
    expect(toast.success).toHaveBeenCalledWith("Backup downloaded");
  });

  it("surfaces a decrypt failure when previewing an import", async () => {
    backupService.previewImport.mockRejectedValue(
      new WardnetApiError(400, "Bad Request", {
        error: "decrypt_failed",
        detail: "Wrong passphrase",
      }),
    );
    const { result } = renderHook(() => usePreviewImport(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current
        .mutateAsync({ bundle: new Blob(["b"]), passphrase: "bad" })
        .catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith("Wrong passphrase");
  });

  it("applies an import and reports the restart-required toast", async () => {
    backupService.applyImport.mockResolvedValue({ ok: true });
    const { result } = renderHook(() => useApplyImport(), {
      wrapper: wrapper(),
    });
    await act(async () => {
      await result.current.mutateAsync({ token: "t" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith(
      "Backup restored - restart the daemon to complete",
    );
  });
});
