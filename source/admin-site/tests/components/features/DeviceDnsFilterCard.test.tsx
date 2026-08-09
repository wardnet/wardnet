import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceDnsFilterCard } from "@/components/features/DeviceDnsFilterCard";
import { makeDevice, renderWithProviders } from "../../test-utils";
import type {
  DeviceDnsFilterSettings,
  DnsFilterConfig,
  DnsFilterProfile,
} from "@wardnet/js";

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

function cardProps({
  settings = { enabled: true, profile_ids: [] as string[] },
  defaultIds = [] as string[],
  loading = false,
  update = {},
}: {
  settings?: { enabled: boolean; profile_ids: string[] } | undefined;
  defaultIds?: string[];
  loading?: boolean;
  update?: Partial<{
    isPending: boolean;
    isError: boolean;
    error: Error | null;
  }>;
} = {}) {
  return {
    device: makeDevice(),
    settings: settings as DeviceDnsFilterSettings | undefined,
    profiles,
    config: {
      enabled: true,
      default_profile_ids: defaultIds,
    } as DnsFilterConfig,
    isLoading: loading,
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

describe("DeviceDnsFilterCard read view", () => {
  it("shows a loading placeholder until every query settles", () => {
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({ settings: undefined, loading: true })}
      />,
    );
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("lists assigned profiles by name", () => {
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({ settings: { enabled: true, profile_ids: ["p1"] } })}
      />,
    );
    expect(screen.getByText("Filtering on")).toBeInTheDocument();
    expect(screen.getByText("Ads")).toBeInTheDocument();
  });

  it("shows the default profile hint when none are assigned", () => {
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({
          settings: { enabled: true, profile_ids: [] },
          defaultIds: ["p2"],
        })}
      />,
    );
    expect(screen.getByText("Malware (default)")).toBeInTheDocument();
  });

  it("shows the no-default message", () => {
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({
          settings: { enabled: true, profile_ids: [] },
          defaultIds: [],
        })}
      />,
    );
    expect(
      screen.getByText("None (no default profile set)"),
    ).toBeInTheDocument();
  });

  it("shows a dash when filtering is off", () => {
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({ settings: { enabled: false, profile_ids: [] } })}
      />,
    );
    expect(screen.getByText("Filtering off")).toBeInTheDocument();
  });
});

describe("DeviceDnsFilterCard editing", () => {
  it("saves updated settings", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({ settings: { enabled: true, profile_ids: ["p1"] } })}
      />,
    );
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
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({
          settings: { enabled: true, profile_ids: [] },
          defaultIds: ["p1"],
        })}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(
      screen.getByText(/follows the global default profile/),
    ).toBeInTheDocument();
  });

  it("shows the unfiltered hint when no default and nothing selected", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({
          settings: { enabled: true, profile_ids: [] },
          defaultIds: [],
        })}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    expect(
      screen.getByText(/this device's traffic\s+isn't filtered/),
    ).toBeInTheDocument();
  });

  it("shows the filtering-off hint after disabling the toggle", async () => {
    const user = userEvent.setup();
    renderWithProviders(
      <DeviceDnsFilterCard
        {...cardProps({
          settings: { enabled: true, profile_ids: [] },
          defaultIds: [],
        })}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("switch", { name: /DNS filtering/i }));
    expect(screen.getByText(/skip every profile/)).toBeInTheDocument();
  });

  it("cancels editing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DeviceDnsFilterCard {...cardProps()} />);
    await user.click(screen.getByRole("button", { name: "Edit" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
    expect(reset).toHaveBeenCalled();
  });

  it("renders an error alert while pending", async () => {
    const user = userEvent.setup();
    const { rerender } = renderWithProviders(
      <DeviceDnsFilterCard {...cardProps()} />,
    );
    await user.click(screen.getByRole("button", { name: "Edit" }));
    rerender(
      <DeviceDnsFilterCard
        {...cardProps({
          update: { isPending: true, isError: true, error: new Error("x") },
        })}
      />,
    );
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Saving…" })).toBeInTheDocument();
  });
});
