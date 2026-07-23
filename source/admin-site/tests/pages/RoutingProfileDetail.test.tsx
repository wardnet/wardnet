import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

const {
  useRoutingProfile,
  useDomainRoutingRules,
  useCreateDomainRoutingRule,
  useUpdateDomainRoutingRule,
  useDeleteDomainRoutingRule,
  useProfileDevices,
  useTunnels,
  useDevices,
} = vi.hoisted(() => ({
  useRoutingProfile: vi.fn(),
  useDomainRoutingRules: vi.fn(),
  useCreateDomainRoutingRule: vi.fn(),
  useUpdateDomainRoutingRule: vi.fn(),
  useDeleteDomainRoutingRule: vi.fn(),
  useProfileDevices: vi.fn(),
  useTunnels: vi.fn(),
  useDevices: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useParams: () => ({ id: "p1" }) };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useRoutingProfile,
    useDomainRoutingRules,
    useCreateDomainRoutingRule,
    useUpdateDomainRoutingRule,
    useDeleteDomainRoutingRule,
    useProfileDevices,
    useTunnels,
    useDevices,
  };
});

import RoutingProfileDetail from "@/pages/RoutingProfileDetail";
import { renderWithProviders, makeDevice } from "../test-utils";

const createMutateAsync = vi.fn().mockResolvedValue({ rule: { id: "r9" } });
const updateMutate = vi.fn();
const deleteMutateAsync = vi.fn().mockResolvedValue({ message: "gone" });

const rule = {
  id: "r1",
  profile_id: "p1",
  pattern: "*.netflix.com",
  target: { type: "direct" as const },
  enabled: true,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

function setup(rules = [rule], deviceIds: string[] = []) {
  useRoutingProfile.mockReturnValue({
    data: { profile: { id: "p1", name: "Streaming" } },
    isLoading: false,
    isError: false,
  });
  useDomainRoutingRules.mockReturnValue({ data: { rules } });
  useCreateDomainRoutingRule.mockReturnValue({
    mutateAsync: createMutateAsync,
    isPending: false,
    error: null,
  });
  useUpdateDomainRoutingRule.mockReturnValue({
    mutate: updateMutate,
    mutateAsync: vi.fn().mockResolvedValue({ rule }),
    isPending: false,
    error: null,
  });
  useDeleteDomainRoutingRule.mockReturnValue({
    mutateAsync: deleteMutateAsync,
    isPending: false,
    error: null,
  });
  useProfileDevices.mockReturnValue({
    data: { device_ids: deviceIds },
    isLoading: false,
  });
  useTunnels.mockReturnValue({ data: { tunnels: [] } });
  useDevices.mockReturnValue({
    data: { devices: [makeDevice({ id: "dev-1", name: "Laptop" })] },
  });
  renderWithProviders(<RoutingProfileDetail />);
}

describe("RoutingProfileDetail", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists a rule with its pattern and target label", () => {
    setup();
    expect(screen.getByText("*.netflix.com")).toBeInTheDocument();
    expect(screen.getByText("Direct (no VPN)")).toBeInTheDocument();
  });

  it("toggles a rule's enabled flag", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("switch"));
    expect(updateMutate).toHaveBeenCalledWith({
      ruleId: "r1",
      body: { enabled: false },
    });
  });

  it("adds a rule with a direct target", async () => {
    const user = userEvent.setup();
    setup([]);
    await user.click(screen.getByTestId("routing-rule-add"));
    await user.type(screen.getByTestId("routing-rule-pattern"), "netflix.com");
    await user.click(screen.getByTestId("routing-rule-save"));
    expect(createMutateAsync).toHaveBeenCalledWith({
      profileId: "p1",
      body: {
        pattern: "netflix.com",
        target: { type: "direct" },
        enabled: true,
      },
    });
  });

  it("lists assigned devices under 'Used by' using the shared device row", () => {
    setup([rule], ["dev-1"]);
    // Rendered through the same HostCell/DeviceIcon/deviceDisplayName path as
    // the tunnel "used by" table: the device's display name and IP both show.
    expect(screen.getByText("Laptop")).toBeInTheDocument();
    expect(screen.getByText("10.232.1.10")).toBeInTheDocument();
  });
});
