import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useRuleRequests, useDevices, useDecideRuleRequest } = vi.hoisted(
  () => ({
    useRuleRequests: vi.fn(),
    useDevices: vi.fn(),
    useDecideRuleRequest: vi.fn(),
  }),
);

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useRuleRequests, useDevices, useDecideRuleRequest };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));

import RuleRequests from "@/pages/RuleRequests";
import { makeDevice, renderWithProviders } from "../test-utils";

const decideMutate = vi.fn();

function req(overrides: Record<string, unknown> = {}) {
  return {
    id: "req-1",
    device_id: "dev-1",
    kind: "block",
    domain: "ads.example.com",
    status: "pending",
    reason: "annoying ads",
    created_at: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  useDecideRuleRequest.mockReturnValue({
    mutate: decideMutate,
    isPending: false,
    isError: false,
    error: null,
  });
  useDevices.mockReturnValue({
    data: { devices: [makeDevice({ id: "dev-1", name: "Kids iPad" })] },
  });
  useRuleRequests.mockReturnValue({
    data: [],
    isLoading: false,
    isError: false,
    error: null,
  });
});

describe("RuleRequests", () => {
  it("shows loading state", () => {
    useRuleRequests.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
      error: null,
    });
    renderWithProviders(<RuleRequests />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows error state", () => {
    useRuleRequests.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: {},
    });
    renderWithProviders(<RuleRequests />);
    expect(
      screen.getByText("Failed to load rule requests"),
    ).toBeInTheDocument();
  });

  it("shows empty state when no pending requests", () => {
    renderWithProviders(<RuleRequests />);
    expect(screen.getByText("No requests.")).toBeInTheDocument();
  });

  it("renders a pending block request with tab counts", () => {
    useRuleRequests.mockReturnValue({
      data: [
        req({ id: "a", status: "pending", kind: "block" }),
        req({ id: "b", status: "approved", kind: "allow", reason: null }),
        req({ id: "c", status: "rejected" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<RuleRequests />);
    expect(screen.getByText("Block request")).toBeInTheDocument();
    expect(screen.getByText("ads.example.com")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Kids iPad · " + new Date("2026-01-01T00:00:00Z").toLocaleString(),
      ),
    ).toBeInTheDocument();
    // Approve/Reject shown for pending row.
    expect(screen.getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reject" })).toBeInTheDocument();
  });

  it("approves and rejects a pending request", async () => {
    useRuleRequests.mockReturnValue({
      data: [req({ id: "a", status: "pending" })],
      isLoading: false,
      isError: false,
      error: null,
    });
    const user = userEvent.setup();
    renderWithProviders(<RuleRequests />);
    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(decideMutate).toHaveBeenCalledWith({ id: "a", status: "approved" });
    await user.click(screen.getByRole("button", { name: "Reject" }));
    expect(decideMutate).toHaveBeenCalledWith({ id: "a", status: "rejected" });
  });

  it("switches tabs to show approved and all requests", async () => {
    useRuleRequests.mockReturnValue({
      data: [
        req({ id: "a", status: "pending" }),
        req({ id: "b", status: "approved", kind: "allow" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    const user = userEvent.setup();
    renderWithProviders(<RuleRequests />);
    // Default pending filter shows the block request.
    expect(screen.getByText("Block request")).toBeInTheDocument();
    // Switch to Approved.
    await user.click(screen.getByRole("tab", { name: /Approved/ }));
    expect(screen.getByText("Allow request")).toBeInTheDocument();
    // Switch to All.
    await user.click(screen.getByRole("tab", { name: /All/ }));
    expect(screen.getByText("Block request")).toBeInTheDocument();
    expect(screen.getByText("Allow request")).toBeInTheDocument();
  });

  it("labels an unknown device", () => {
    useRuleRequests.mockReturnValue({
      data: [
        req({ id: "a", device_id: "missing-device-id", status: "pending" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<RuleRequests />);
    expect(screen.getByText(/Unknown device/)).toBeInTheDocument();
  });

  it("shows a decide error alert inside the row", () => {
    useDecideRuleRequest.mockReturnValue({
      mutate: decideMutate,
      isPending: false,
      isError: true,
      error: {},
    });
    useRuleRequests.mockReturnValue({
      data: [req({ id: "a", status: "pending" })],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<RuleRequests />);
    expect(screen.getByText("Failed to update request")).toBeInTheDocument();
  });
});
