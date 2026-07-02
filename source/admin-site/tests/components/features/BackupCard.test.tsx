import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { WardnetApiError, type RestorePreviewResponse } from "@wardnet/js";
import { BackupCard } from "@/components/features/BackupCard";
import { renderWithProviders } from "../../test-utils";

function baseProps() {
  return {
    isExporting: false,
    isPreviewing: false,
    isApplying: false,
    preview: null as RestorePreviewResponse | null,
    onExport: vi.fn(),
    onPreview: vi.fn(),
    onApply: vi.fn(),
    onDismissPreview: vi.fn(),
  };
}

function makePreview(
  overrides: Partial<RestorePreviewResponse> = {},
): RestorePreviewResponse {
  return {
    preview_token: "tok-123",
    compatible: true,
    incompatibility_reason: null,
    files_to_replace: ["/var/lib/wardnet/db.sqlite"],
    manifest: {
      wardnet_version: "1.2.3",
      host_id: "host-abc",
      created_at: "2026-01-01T00:00:00Z",
      schema_version: 42,
      key_count: 3,
    },
    ...overrides,
  } as unknown as RestorePreviewResponse;
}

describe("BackupCard", () => {
  it("shows the collapsed backup + restore descriptions", () => {
    renderWithProviders(<BackupCard {...baseProps()} />);
    expect(screen.getByTestId("backup-export-open")).toBeInTheDocument();
    expect(screen.getByTestId("backup-restore-open")).toBeInTheDocument();
    expect(
      screen.getByText(/Keep the passphrase somewhere safe/),
    ).toBeInTheDocument();
  });

  it("exports after entering matching passphrases", async () => {
    const user = userEvent.setup();
    const onExport = vi.fn();
    renderWithProviders(<BackupCard {...baseProps()} onExport={onExport} />);
    await user.click(screen.getByTestId("backup-export-open"));
    await user.type(screen.getByTestId("backup-passphrase"), "correcthorse1");
    await user.type(
      screen.getByTestId("backup-passphrase-confirm"),
      "correcthorse1",
    );
    await user.click(screen.getByTestId("backup-export-submit"));
    expect(onExport).toHaveBeenCalledWith("correcthorse1");
  });

  it("blocks export when the confirmation does not match", async () => {
    const user = userEvent.setup();
    const onExport = vi.fn();
    renderWithProviders(<BackupCard {...baseProps()} onExport={onExport} />);
    await user.click(screen.getByTestId("backup-export-open"));
    await user.type(screen.getByTestId("backup-passphrase"), "correcthorse1");
    await user.type(
      screen.getByTestId("backup-passphrase-confirm"),
      "different1234",
    );
    await user.click(screen.getByTestId("backup-export-submit"));
    expect(onExport).not.toHaveBeenCalled();
    expect(screen.getByText("Passphrases do not match.")).toBeInTheDocument();
  });

  it("shows the exporting spinner label", () => {
    renderWithProviders(<BackupCard {...baseProps()} isExporting />);
    // Export form stays collapsed unless opened; open state is driven by
    // internal state, but the export button is disabled while exporting.
    expect(screen.getByTestId("backup-export-open")).toBeDisabled();
  });

  it("renders an export error inline", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <BackupCard {...baseProps()} exportError={new Error("boom")} />,
    );
    await user.click(screen.getByTestId("backup-export-open"));
    expect(screen.getByText("boom")).toBeInTheDocument();
  });

  it("previews a bundle after selecting a file and passphrase", async () => {
    const user = userEvent.setup();
    const onPreview = vi.fn();
    renderWithProviders(<BackupCard {...baseProps()} onPreview={onPreview} />);
    await user.click(screen.getByTestId("backup-restore-open"));
    const file = new File(["data"], "backup.wardnet.age", {
      type: "application/octet-stream",
    });
    await user.upload(screen.getByTestId("backup-bundle"), file);
    await user.type(screen.getByTestId("restore-passphrase"), "correcthorse1");
    await user.click(screen.getByTestId("backup-preview-submit"));
    expect(onPreview).toHaveBeenCalledWith({
      bundle: file,
      passphrase: "correcthorse1",
    });
  });

  it("blocks preview when no bundle is selected", async () => {
    const user = userEvent.setup();
    const onPreview = vi.fn();
    renderWithProviders(<BackupCard {...baseProps()} onPreview={onPreview} />);
    await user.click(screen.getByTestId("backup-restore-open"));
    await user.type(screen.getByTestId("restore-passphrase"), "correcthorse1");
    await user.click(screen.getByTestId("backup-preview-submit"));
    expect(onPreview).not.toHaveBeenCalled();
    expect(
      screen.getByText("Select a backup bundle to restore."),
    ).toBeInTheDocument();
  });

  it("renders the API-provided detail for a decrypt failure", async () => {
    const user = userEvent.setup();
    // WardnetApiError with a decrypt detail exercises restoreErrorMessage's
    // ApiError branch; ApiErrorAlert surfaces the server detail directly.
    const err = new WardnetApiError(400, "Bad Request", {
      error: "restore failed",
      detail: "decrypt failed",
    });
    renderWithProviders(<BackupCard {...baseProps()} previewError={err} />);
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByText("decrypt failed")).toBeInTheDocument();
  });

  it("falls back to the friendly message for a non-API error", async () => {
    const user = userEvent.setup();
    // A plain (non-Error, non-ApiError) value exercises restoreErrorMessage's
    // fallback return, which ApiErrorAlert then displays.
    renderWithProviders(
      <BackupCard {...baseProps()} previewError={{ some: "thing" }} />,
    );
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByText("Couldn't open the bundle.")).toBeInTheDocument();
  });

  it("renders the preview details and applies on confirm", async () => {
    const user = userEvent.setup();
    const onApply = vi.fn();
    renderWithProviders(
      <BackupCard {...baseProps()} preview={makePreview()} onApply={onApply} />,
    );
    // preview forces the restore card into preview state after opening.
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByTestId("restore-preview")).toBeInTheDocument();
    expect(screen.getByText("host-abc")).toBeInTheDocument();
    expect(screen.getByText("/var/lib/wardnet/db.sqlite")).toBeInTheDocument();
    await user.click(screen.getByTestId("backup-apply-submit"));
    expect(onApply).toHaveBeenCalledWith("tok-123");
  });

  it("cancels out of the export form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<BackupCard {...baseProps()} />);
    await user.click(screen.getByTestId("backup-export-open"));
    expect(screen.getByTestId("backup-passphrase")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("backup-passphrase")).not.toBeInTheDocument();
  });

  it("cancels out of the restore form and dismisses the preview", async () => {
    const user = userEvent.setup();
    const onDismissPreview = vi.fn();
    renderWithProviders(
      <BackupCard {...baseProps()} onDismissPreview={onDismissPreview} />,
    );
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByTestId("restore-passphrase")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("restore-passphrase")).not.toBeInTheDocument();
    expect(onDismissPreview).toHaveBeenCalled();
  });

  it("collapses the export form after a successful export settles", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(<BackupCard {...baseProps()} />);
    await user.click(screen.getByTestId("backup-export-open"));
    expect(screen.getByTestId("backup-passphrase")).toBeInTheDocument();
    // Export starts, then settles with no error → form auto-collapses.
    rerender(<BackupCard {...baseProps()} isExporting />);
    rerender(<BackupCard {...baseProps()} isExporting={false} />);
    expect(screen.queryByTestId("backup-passphrase")).not.toBeInTheDocument();
  });

  it("collapses the restore form after a successful apply settles", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(
      <BackupCard {...baseProps()} preview={makePreview()} />,
    );
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByTestId("restore-preview")).toBeInTheDocument();
    rerender(
      <BackupCard {...baseProps()} preview={makePreview()} isApplying />,
    );
    rerender(
      <BackupCard
        {...baseProps()}
        preview={makePreview()}
        isApplying={false}
      />,
    );
    expect(screen.getByTestId("backup-restore-open")).toBeInTheDocument();
  });

  it("disables apply and shows the incompatibility reason", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <BackupCard
        {...baseProps()}
        preview={makePreview({
          compatible: false,
          incompatibility_reason: "schema too new",
        })}
      />,
    );
    await user.click(screen.getByTestId("backup-restore-open"));
    expect(screen.getByText("Bundle incompatible")).toBeInTheDocument();
    expect(screen.getByText("schema too new")).toBeInTheDocument();
    expect(screen.getByTestId("backup-apply-submit")).toBeDisabled();
  });
});
