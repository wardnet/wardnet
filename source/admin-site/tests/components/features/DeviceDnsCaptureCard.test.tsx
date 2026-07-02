import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceDnsCaptureCard } from "@/components/features/DeviceDnsCaptureCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsCaptureSettings: vi.fn(),
    useUpdateDnsCaptureSettings: vi.fn(),
  };
});

import {
  useDnsCaptureSettings,
  useUpdateDnsCaptureSettings,
} from "@wardnet/web";

const mutateAsync = vi.fn();
const reset = vi.fn();

const defaultData = {
  enabled: true,
  cap_count: 1000,
  cap_days: 7,
  row_count: 950,
  size_bytes: 4096,
};

function setup({
  data = defaultData,
  isLoading = false,
  update = {},
}: {
  data?: Record<string, unknown> | undefined;
  isLoading?: boolean;
  update?: Record<string, unknown>;
} = {}) {
  vi.mocked(useDnsCaptureSettings).mockReturnValue({
    data,
    isLoading,
  } as unknown as ReturnType<typeof useDnsCaptureSettings>);
  vi.mocked(useUpdateDnsCaptureSettings).mockReturnValue({
    mutateAsync,
    reset,
    isPending: false,
    isError: false,
    error: null,
    ...update,
  } as unknown as ReturnType<typeof useUpdateDnsCaptureSettings>);
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
  setup();
});

describe("DeviceDnsCaptureCard", () => {
  it("shows the loading placeholder", () => {
    setup({ data: undefined, isLoading: true });
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders the read-only summary (near-full storage bar branch)", () => {
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
    expect(screen.getByText("Enabled")).toBeInTheDocument();
    expect(screen.getByText(/1,000 records · 7 days/)).toBeInTheDocument();
    expect(screen.getByText(/950 records/)).toBeInTheDocument();
  });

  it("shows Disabled when capture is off", () => {
    setup({ data: { ...defaultData, enabled: false, row_count: 10 } });
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
    expect(screen.getByText("Disabled")).toBeInTheDocument();
  });

  it("enters edit mode, edits fields, and saves", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
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
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });

  it("shows a saving label and error alert while updating", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsCaptureCard deviceId="dev-1" />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    setup({
      update: { isPending: true, isError: true, error: new Error("x") },
    });
    // re-render by toggling the enable switch which triggers state update
    await user.click(screen.getByRole("switch"));
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
