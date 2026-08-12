import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useDnsFilterProfiles,
  useDnsFilterConfig,
  useUpdateDnsFilterConfig,
  useBlocklists,
  useAllowlist,
  useFilterRules,
  navigate,
} = vi.hoisted(() => ({
  useDnsFilterProfiles: vi.fn(),
  useDnsFilterConfig: vi.fn(),
  useUpdateDnsFilterConfig: vi.fn(),
  useBlocklists: vi.fn(),
  useAllowlist: vi.fn(),
  useFilterRules: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsFilterProfiles,
    useDnsFilterConfig,
    useUpdateDnsFilterConfig,
    useBlocklists,
    useAllowlist,
    useFilterRules,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({
    title,
    actions,
  }: {
    title: ReactNode;
    actions?: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      {actions}
    </div>
  ),
}));
vi.mock("@/components/features/DnsFilterSettingsCard", () => ({
  DnsFilterSettingsCard: ({
    onThresholdSave,
  }: {
    onThresholdSave: (threshold: number) => void;
  }) => (
    <div>
      settings-card
      <button
        data-testid="stub-save-threshold"
        onClick={() => onThresholdSave(9)}
      >
        save threshold
      </button>
    </div>
  ),
}));
vi.mock("@/components/compound/EmptyStatePlaceholder", () => ({
  EmptyStatePlaceholder: ({
    message,
    onAction,
  }: {
    message: ReactNode;
    onAction: () => void;
  }) => (
    <div>
      <span>{message}</span>
      <button onClick={onAction}>empty-add</button>
    </div>
  ),
}));

import DnsFilter from "@/pages/DnsFilter";
import { renderWithProviders } from "../test-utils";

const configMutate = vi.fn();

const profile = (over: Record<string, unknown> = {}) => ({
  id: "p1",
  name: "Family",
  description: "the default profile",
  builtin: false,
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
  useUpdateDnsFilterConfig.mockReturnValue({
    mutate: configMutate,
    isPending: false,
  });
  useDnsFilterConfig.mockReturnValue({
    data: { config: { default_profile_ids: ["p1"], enabled: true } },
  });
  useBlocklists.mockReturnValue({
    data: { blocklists: [{ id: "b1", entry_count: 1200 }] },
  });
  useAllowlist.mockReturnValue({ data: { entries: [{ id: "a1" }] } });
  useFilterRules.mockReturnValue({ data: { rules: [{ id: "r1" }] } });
  useDnsFilterProfiles.mockReturnValue({
    data: { profiles: [profile()] },
    isLoading: false,
  });
});

describe("DnsFilter", () => {
  it("shows the loading card", () => {
    useDnsFilterProfiles.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<DnsFilter />);
    expect(screen.getByText("Loading profiles…")).toBeInTheDocument();
  });

  it("shows the empty state and can add from it", async () => {
    useDnsFilterProfiles.mockReturnValue({
      data: { profiles: [] },
      isLoading: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsFilter />);
    expect(screen.getByText("No DNS filter profiles")).toBeInTheDocument();
    await user.click(screen.getByText("empty-add"));
    expect(navigate).toHaveBeenCalledWith("/dns/filter/profiles/new");
  });

  it("renders the profile table with count badges", () => {
    renderWithProviders(<DnsFilter />);
    expect(screen.getByText("Family")).toBeInTheDocument();
    expect(screen.getByText("1,200 blocked domains")).toBeInTheDocument();
    expect(screen.getByText("1 allowed domains")).toBeInTheDocument();
    expect(screen.getByText("1 custom rules")).toBeInTheDocument();
  });

  it("navigates to a profile on row click and via the Add button", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilter />);
    await user.click(screen.getByTestId("filter-add-profile"));
    expect(navigate).toHaveBeenCalledWith("/dns/filter/profiles/new");

    await user.click(screen.getByText("Family"));
    expect(navigate).toHaveBeenCalledWith("/dns/filter/profiles/p1");
  });

  it("toggles a profile in/out of the default set", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilter />);
    // p1 is currently a default → toggling removes it.
    await user.click(
      screen.getByRole("switch", {
        name: /Use Family as a default profile/i,
      }),
    );
    expect(configMutate).toHaveBeenCalledWith({ default_profile_ids: [] });
  });

  it("composes the default set from the current config when toggling a second profile", async () => {
    // p1 is already default; the payload for toggling p2 on must keep p1 —
    // it recomputes from the config the (optimistically-updated) cache holds,
    // not from a copy that drops the earlier still-settling toggle.
    useDnsFilterConfig.mockReturnValue({
      data: { config: { default_profile_ids: ["p1"], enabled: true } },
    });
    useDnsFilterProfiles.mockReturnValue({
      data: {
        profiles: [profile(), profile({ id: "p2", name: "Guests" })],
      },
      isLoading: false,
    });
    const user = userEvent.setup();
    renderWithProviders(<DnsFilter />);

    await user.click(
      screen.getByRole("switch", {
        name: /Use Guests as a default profile/i,
      }),
    );
    expect(configMutate).toHaveBeenCalledWith({
      default_profile_ids: ["p1", "p2"],
    });
  });

  it("shows a Builtin pill and omits zero-count badges", () => {
    useBlocklists.mockReturnValue({ data: { blocklists: [] } });
    useAllowlist.mockReturnValue({ data: { entries: [] } });
    useFilterRules.mockReturnValue({ data: { rules: [] } });
    useDnsFilterProfiles.mockReturnValue({
      data: {
        profiles: [
          profile({ id: "p2", name: "Base", builtin: true, description: null }),
        ],
      },
      isLoading: false,
    });
    renderWithProviders(<DnsFilter />);
    expect(screen.getByText("Builtin")).toBeInTheDocument();
    expect(screen.queryByText(/blocked domains/)).not.toBeInTheDocument();
  });

  it("disables the default toggle when filtering is off", () => {
    useDnsFilterConfig.mockReturnValue({
      data: { config: { default_profile_ids: [], enabled: false } },
    });
    renderWithProviders(<DnsFilter />);
    const row = screen.getByText("Family").closest("tr")!;
    expect(within(row).getByRole("switch")).toBeDisabled();
  });

  it("saves a new blocklist failure-alert threshold through the config mutation", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilter />);

    await user.click(screen.getByTestId("stub-save-threshold"));

    // Only the threshold is sent: the kill switch and the default-profile set
    // share this mutation, so a full-object write would clobber them.
    expect(configMutate).toHaveBeenCalledWith({
      blocklist_failure_alert_threshold: 9,
    });
  });
});
