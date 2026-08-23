import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useAccessRequests, useDevices, useDecideAccessRequest } = vi.hoisted(
  () => ({
    useAccessRequests: vi.fn(),
    useDevices: vi.fn(),
    useDecideAccessRequest: vi.fn(),
  }),
);

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAccessRequests, useDevices, useDecideAccessRequest };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({ title }: { title: ReactNode }) => <h1>{title}</h1>,
}));

import AccessRequests from "@/pages/AccessRequests";
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
  useDecideAccessRequest.mockReturnValue({
    mutate: decideMutate,
    isPending: false,
    isError: false,
    error: null,
  });
  useDevices.mockReturnValue({
    data: { devices: [makeDevice({ id: "dev-1", name: "Kids iPad" })] },
  });
  useAccessRequests.mockReturnValue({
    data: [],
    isLoading: false,
    isError: false,
    error: null,
  });
});

describe("AccessRequests", () => {
  it("shows loading state", () => {
    useAccessRequests.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
      error: null,
    });
    renderWithProviders(<AccessRequests />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("shows error state", () => {
    useAccessRequests.mockReturnValue({
      data: undefined,
      isLoading: false,
      isError: true,
      error: {},
    });
    renderWithProviders(<AccessRequests />);
    expect(
      screen.getByText("Failed to load access requests"),
    ).toBeInTheDocument();
  });

  it("shows empty state when no pending requests", () => {
    renderWithProviders(<AccessRequests />);
    expect(screen.getByText("No requests.")).toBeInTheDocument();
  });

  it("renders a pending block request with tab counts", () => {
    useAccessRequests.mockReturnValue({
      data: [
        req({ id: "a", status: "pending", kind: "block" }),
        req({ id: "b", status: "approved", kind: "allow", reason: null }),
        req({ id: "c", status: "rejected" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<AccessRequests />);
    expect(screen.getByText("Block request")).toBeInTheDocument();
    expect(screen.getByText("ads.example.com")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Kids iPad · " + new Date("2026-01-01T00:00:00Z").toLocaleString(),
      ),
    ).toBeInTheDocument();
    // Approve/Decline shown for pending row.
    expect(screen.getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Decline" })).toBeInTheDocument();
  });

  it("approves and declines a pending request", async () => {
    useAccessRequests.mockReturnValue({
      data: [req({ id: "a", status: "pending" })],
      isLoading: false,
      isError: false,
      error: null,
    });
    const user = userEvent.setup();
    renderWithProviders(<AccessRequests />);
    await user.click(screen.getByRole("button", { name: "Approve" }));
    // A rule request carries no approval params — that kind needs no admin
    // input, and the auto-apply follow-up is what will add some.
    expect(decideMutate).toHaveBeenCalledWith({
      id: "a",
      status: "approved",
      approval: undefined,
    });
    await user.click(screen.getByRole("button", { name: "Decline" }));
    expect(decideMutate).toHaveBeenCalledWith({ id: "a", status: "rejected" });
  });

  it("renders a Private DNS request with no domain and its own approval params", async () => {
    useAccessRequests.mockReturnValue({
      data: [
        req({
          id: "p",
          status: "pending",
          kind: "private_dns",
          domain: null,
          reason: null,
        }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    const user = userEvent.setup();
    renderWithProviders(<AccessRequests />);

    expect(screen.getByText("Private DNS request")).toBeInTheDocument();
    // The device is the subject, so no domain line is rendered.
    expect(screen.queryByText("ads.example.com")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Approve" }));
    expect(decideMutate).toHaveBeenCalledWith({
      id: "p",
      status: "approved",
      approval: { kind: "private_dns" },
    });
  });

  it("switches tabs to show approved and all requests", async () => {
    useAccessRequests.mockReturnValue({
      data: [
        req({ id: "a", status: "pending" }),
        req({ id: "b", status: "approved", kind: "allow" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    const user = userEvent.setup();
    renderWithProviders(<AccessRequests />);
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
    useAccessRequests.mockReturnValue({
      data: [
        req({ id: "a", device_id: "missing-device-id", status: "pending" }),
      ],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<AccessRequests />);
    expect(screen.getByText(/Unknown device/)).toBeInTheDocument();
  });

  it("shows a decide error alert inside the row", () => {
    useDecideAccessRequest.mockReturnValue({
      mutate: decideMutate,
      isPending: false,
      isError: true,
      error: {},
    });
    useAccessRequests.mockReturnValue({
      data: [req({ id: "a", status: "pending" })],
      isLoading: false,
      isError: false,
      error: null,
    });
    renderWithProviders(<AccessRequests />);
    expect(screen.getByText("Failed to update request")).toBeInTheDocument();
  });
});
