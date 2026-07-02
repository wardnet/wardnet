import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useCreateDnsFilterProfile, navigate } = vi.hoisted(() => ({
  useCreateDnsFilterProfile: vi.fn(),
  navigate: vi.fn(),
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return { ...actual, useNavigate: () => navigate };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useCreateDnsFilterProfile };
});

vi.mock("@/components/compound/DetailPageHeader", () => ({
  DetailPageHeader: ({ itemLabel }: { itemLabel: ReactNode }) => (
    <div>header:{itemLabel}</div>
  ),
}));

import DnsFilterProfileNew from "@/pages/DnsFilterProfileNew";
import { renderWithProviders } from "../test-utils";

const mutateAsync = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useCreateDnsFilterProfile.mockReturnValue({
    mutateAsync,
    isPending: false,
    isError: false,
    error: null,
  });
});

describe("DnsFilterProfileNew", () => {
  it("renders the form with a disabled submit until a name is typed", () => {
    renderWithProviders(<DnsFilterProfileNew />);
    expect(screen.getByText("header:New profile")).toBeInTheDocument();
    expect(screen.getByTestId("profile-create-submit")).toBeDisabled();
  });

  it("creates the profile and navigates to its detail page", async () => {
    mutateAsync.mockResolvedValue({ profile: { id: "prof-9" } });
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfileNew />);

    await user.type(screen.getByTestId("profile-name"), "Kids");
    await user.type(screen.getByLabelText(/Description/i), "strict");
    await user.click(screen.getByTestId("profile-create-submit"));

    expect(mutateAsync).toHaveBeenCalledWith({
      name: "Kids",
      description: "strict",
    });
    expect(navigate).toHaveBeenCalledWith("/dns/filter/profiles/prof-9");
  });

  it("omits an empty description", async () => {
    mutateAsync.mockResolvedValue({ profile: { id: "p1" } });
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfileNew />);
    await user.type(screen.getByTestId("profile-name"), "Solo");
    await user.click(screen.getByTestId("profile-create-submit"));
    expect(mutateAsync).toHaveBeenCalledWith({
      name: "Solo",
      description: undefined,
    });
  });

  it("navigates back on cancel", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfileNew />);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(navigate).toHaveBeenCalledWith("/dns/filter");
  });

  it("shows an error alert and disables inputs while pending", () => {
    useCreateDnsFilterProfile.mockReturnValue({
      mutateAsync,
      isPending: true,
      isError: true,
      error: new Error("boom"),
    });
    renderWithProviders(<DnsFilterProfileNew />);
    expect(screen.getByText("boom")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Creating…" })).toBeDisabled();
  });
});
