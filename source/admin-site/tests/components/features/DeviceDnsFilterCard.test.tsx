import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceDnsFilterCard } from "@/components/features/DeviceDnsFilterCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type { DnsFilterProfile } from "@wardnet/js";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDeviceFilterSettings: vi.fn(),
    useDnsFilterProfiles: vi.fn(),
    useDnsFilterConfig: vi.fn(),
    useUpdateDeviceFilterSettings: vi.fn(),
  };
});

import {
  useDeviceFilterSettings,
  useDnsFilterConfig,
  useDnsFilterProfiles,
  useUpdateDeviceFilterSettings,
} from "@wardnet/web";

const mutateAsync = vi.fn();
const reset = vi.fn();

function profile(id: string, name: string): DnsFilterProfile {
  return {
    id,
    name,
    description: null,
    builtin: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

const profiles = [profile("p1", "Ads"), profile("p2", "Malware")];

function setup({
  settings = { enabled: true, profile_ids: [] as string[] },
  defaultIds = [] as string[],
  loading = false,
  update = {},
}: {
  settings?: { enabled: boolean; profile_ids: string[] } | undefined;
  defaultIds?: string[];
  loading?: boolean;
  update?: Record<string, unknown>;
} = {}) {
  vi.mocked(useDeviceFilterSettings).mockReturnValue({
    data: settings ? { settings } : undefined,
    isLoading: loading,
  } as unknown as ReturnType<typeof useDeviceFilterSettings>);
  vi.mocked(useDnsFilterProfiles).mockReturnValue({
    data: { profiles },
    isLoading: loading,
  } as unknown as ReturnType<typeof useDnsFilterProfiles>);
  vi.mocked(useDnsFilterConfig).mockReturnValue({
    data: { config: { enabled: true, default_profile_ids: defaultIds } },
    isLoading: loading,
  } as unknown as ReturnType<typeof useDnsFilterConfig>);
  vi.mocked(useUpdateDeviceFilterSettings).mockReturnValue({
    mutateAsync,
    reset,
    isPending: false,
    isError: false,
    error: null,
    ...update,
  } as unknown as ReturnType<typeof useUpdateDeviceFilterSettings>);
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
  setup();
});

describe("DeviceDnsFilterCard read view", () => {
  it("shows a loading placeholder until every query settles", () => {
    setup({ settings: undefined, loading: true });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("lists assigned profiles by name", () => {
    setup({ settings: { enabled: true, profile_ids: ["p1"] } });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    expect(screen.getByText("Filtering on")).toBeInTheDocument();
    expect(screen.getByText("Ads")).toBeInTheDocument();
  });

  it("shows the default profile hint when none are assigned", () => {
    setup({
      settings: { enabled: true, profile_ids: [] },
      defaultIds: ["p2"],
    });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    expect(screen.getByText("Malware (default)")).toBeInTheDocument();
  });

  it("shows the no-default message", () => {
    setup({ settings: { enabled: true, profile_ids: [] }, defaultIds: [] });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    expect(
      screen.getByText("None (no default profile set)"),
    ).toBeInTheDocument();
  });

  it("shows a dash when filtering is off", () => {
    setup({ settings: { enabled: false, profile_ids: [] } });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    expect(screen.getByText("Filtering off")).toBeInTheDocument();
  });
});

describe("DeviceDnsFilterCard editing", () => {
  it("saves updated settings", async () => {
    const user = userEvent.setup();
    setup({ settings: { enabled: true, profile_ids: ["p1"] } });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    // hasExplicit hint (profile already selected)
    expect(
      screen.getByText(/Selected profiles are stacked/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith({
      id: "dev-1",
      body: { enabled: true, profile_ids: ["p1"] },
    });
  });

  it("shows the global-default hint when nothing is selected", async () => {
    const user = userEvent.setup();
    setup({ settings: { enabled: true, profile_ids: [] }, defaultIds: ["p1"] });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(
      screen.getByText(/follows the global default profile/),
    ).toBeInTheDocument();
  });

  it("shows the unfiltered hint when no default and nothing selected", async () => {
    const user = userEvent.setup();
    setup({ settings: { enabled: true, profile_ids: [] }, defaultIds: [] });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(
      screen.getByText(/this device's traffic\s+isn't filtered/),
    ).toBeInTheDocument();
  });

  it("shows the filtering-off hint after disabling the toggle", async () => {
    const user = userEvent.setup();
    setup({ settings: { enabled: true, profile_ids: [] }, defaultIds: [] });
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("switch", { name: /DNS filtering/i }));
    expect(screen.getByText(/skip every profile/)).toBeInTheDocument();
  });

  it("cancels editing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });

  it("renders an error alert while pending", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsFilterCard device={makeDevice()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    setup({
      update: { isPending: true, isError: true, error: new Error("x") },
    });
    await user.click(screen.getByRole("switch", { name: /DNS filtering/i }));
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
  });
});
