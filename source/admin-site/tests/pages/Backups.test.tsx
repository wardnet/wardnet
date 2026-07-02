/* eslint-disable @typescript-eslint/no-explicit-any */
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  navigate,
  useExportBackup,
  usePreviewImport,
  useApplyImport,
  useRestart,
  useAuthStore,
} = vi.hoisted(() => ({
  navigate: vi.fn(),
  useExportBackup: vi.fn(),
  usePreviewImport: vi.fn(),
  useApplyImport: vi.fn(),
  useRestart: vi.fn(),
  useAuthStore: vi.fn(),
}));

vi.mock("react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useExportBackup,
    usePreviewImport,
    useApplyImport,
    useRestart,
    useAuthStore,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: any) => <h1>{title}</h1>,
}));

// A rich stub that lets us drive every callback the page wires up.
vi.mock("@/components/features/BackupCard", () => ({
  BackupCard: (props: any) => (
    <div
      data-testid="backup-card"
      data-preview={props.preview ? "yes" : "no"}
      data-exporting={String(props.isExporting)}
      data-previewing={String(props.isPreviewing)}
      data-applying={String(props.isApplying)}
    >
      <button onClick={() => props.onExport("pw")}>export</button>
      <button onClick={() => props.onPreview({ file: "f", passphrase: "pw" })}>
        preview
      </button>
      <button onClick={() => props.onApply("tok-1")}>apply</button>
      <button onClick={() => props.onDismissPreview()}>dismiss-preview</button>
    </div>
  ),
}));

vi.mock("@/components/features/RestartProgressDialog", () => ({
  RestartProgressDialog: ({ open, onDismiss, onSignIn }: any) =>
    open ? (
      <div data-testid="restart-dialog">
        <button onClick={onDismiss}>dismiss</button>
        <button onClick={onSignIn}>sign-in</button>
      </div>
    ) : null,
}));

import Backups from "@/pages/Backups";
import { renderWithProviders } from "../test-utils";

const logout = vi.fn();
let exportMutate: ReturnType<typeof vi.fn>;
let previewMutate: ReturnType<typeof vi.fn>;
let applyMutate: ReturnType<typeof vi.fn>;
let restart: any;

beforeEach(() => {
  vi.clearAllMocks();
  useAuthStore.mockImplementation((sel: any) => sel({ logout }));
  exportMutate = vi.fn();
  previewMutate = vi.fn();
  applyMutate = vi.fn();
  restart = {
    isOpen: false,
    phase: "idle",
    startedAt: null,
    errorMessage: null,
    start: vi.fn(),
    reset: vi.fn(),
  };
  useExportBackup.mockReturnValue({
    isPending: false,
    error: null,
    mutate: exportMutate,
  });
  usePreviewImport.mockReturnValue({
    isPending: false,
    error: null,
    mutate: previewMutate,
  });
  useApplyImport.mockReturnValue({
    isPending: false,
    error: null,
    mutate: applyMutate,
  });
  useRestart.mockReturnValue(restart);
});

describe("Backups", () => {
  it("renders the backup card with no preview", () => {
    renderWithProviders(<Backups />);
    expect(screen.getByText("Backups")).toBeInTheDocument();
    expect(screen.getByTestId("backup-card")).toHaveAttribute(
      "data-preview",
      "no",
    );
    expect(screen.queryByTestId("restart-dialog")).not.toBeInTheDocument();
  });

  it("exports with the entered passphrase", async () => {
    const user = userEvent.setup();
    renderWithProviders(<Backups />);
    await user.click(screen.getByText("export"));
    expect(exportMutate).toHaveBeenCalledWith({ passphrase: "pw" });
  });

  it("stores the preview response and renders it", async () => {
    previewMutate.mockImplementation((_args, opts) =>
      opts.onSuccess({ preview_token: "tok-1", summary: {} }),
    );
    const user = userEvent.setup();
    renderWithProviders(<Backups />);
    await user.click(screen.getByText("preview"));
    expect(previewMutate).toHaveBeenCalled();
    expect(screen.getByTestId("backup-card")).toHaveAttribute(
      "data-preview",
      "yes",
    );
  });

  it("applies an import, clears the preview, and starts a restart", async () => {
    // Seed a preview first.
    previewMutate.mockImplementation((_args, opts) =>
      opts.onSuccess({ preview_token: "tok-1" }),
    );
    applyMutate.mockImplementation((_args, opts) => opts.onSuccess());
    const user = userEvent.setup();
    renderWithProviders(<Backups />);
    await user.click(screen.getByText("preview"));
    await user.click(screen.getByText("apply"));
    expect(applyMutate).toHaveBeenCalledWith(
      { preview_token: "tok-1" },
      expect.any(Object),
    );
    expect(restart.start).toHaveBeenCalled();
    expect(screen.getByTestId("backup-card")).toHaveAttribute(
      "data-preview",
      "no",
    );
  });

  it("dismisses a stored preview", async () => {
    previewMutate.mockImplementation((_args, opts) =>
      opts.onSuccess({ preview_token: "tok-1" }),
    );
    const user = userEvent.setup();
    renderWithProviders(<Backups />);
    await user.click(screen.getByText("preview"));
    expect(screen.getByTestId("backup-card")).toHaveAttribute(
      "data-preview",
      "yes",
    );
    await user.click(screen.getByText("dismiss-preview"));
    expect(screen.getByTestId("backup-card")).toHaveAttribute(
      "data-preview",
      "no",
    );
  });

  it("dismiss and sign-in drive the restart dialog", async () => {
    useRestart.mockReturnValue({ ...restart, isOpen: true });
    const user = userEvent.setup();
    renderWithProviders(<Backups />);
    await user.click(screen.getByText("dismiss"));
    expect(restart.reset).toHaveBeenCalledTimes(1);
    await user.click(screen.getByText("sign-in"));
    expect(logout).toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith("/login");
  });
});
