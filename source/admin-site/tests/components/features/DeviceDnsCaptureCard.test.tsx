import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceDnsCaptureCard } from "@/components/features/DeviceDnsCaptureCard";
import { renderWithProviders } from "../../test-utils";
import type { DnsCaptureSettingsResponse } from "@wardnet/js";

const mutateAsync = vi.fn();
const reset = vi.fn();

const defaultData: DnsCaptureSettingsResponse = {
  enabled: true,
  cap_count: 1000,
  cap_days: 7,
  row_count: 950,
  size_bytes: 4096,
};

function cardProps({
  settings = defaultData,
  isLoading = false,
  update = {},
}: {
  settings?: DnsCaptureSettingsResponse | undefined;
  isLoading?: boolean;
  update?: Partial<{
    isPending: boolean;
    isError: boolean;
    error: Error | null;
  }>;
} = {}) {
  return {
    settings,
    isLoading,
    update: {
      mutateAsync,
      reset,
      isPending: false,
      isError: false,
      error: null,
      ...update,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
});

describe("DeviceDnsCaptureCard", () => {
  it("shows the loading placeholder", () => {
    renderWithProviders(
      <DeviceDnsCaptureCard
        {...cardProps({ settings: undefined, isLoading: true })}
      />,
    );
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the read-only summary (near-full storage bar branch)", () => {
    renderWithProviders(<DeviceDnsCaptureCard {...cardProps()} />);
    expect(screen.getByText("Enabled")).toBeInTheDocument();
    expect(screen.getByText(/1,000 records · 7 days/)).toBeInTheDocument();
    expect(screen.getByText(/950 records/)).toBeInTheDocument();
  });

  it("shows Disabled when capture is off", () => {
    renderWithProviders(
      <DeviceDnsCaptureCard
        {...cardProps({
          settings: { ...defaultData, enabled: false, row_count: 10 },
        })}
      />,
    );
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("enters edit mode, edits fields, and saves", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsCaptureCard {...cardProps()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));

    const numbers = screen.getAllByRole("spinbutton");
    await user.clear(numbers[0]);
    await user.type(numbers[0], "500");
    await user.clear(numbers[1]);
    await user.type(numbers[1], "3");

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith({
      enabled: true,
      cap_count: 500,
      cap_days: 3,
    });
  });

  it("cancels editing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsCaptureCard {...cardProps()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });

  it("shows a saving label and error alert while updating", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(
      <DeviceDnsCaptureCard {...cardProps()} />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    rerender(
      <DeviceDnsCaptureCard
        {...cardProps({
          update: { isPending: true, isError: true, error: new Error("x") },
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
