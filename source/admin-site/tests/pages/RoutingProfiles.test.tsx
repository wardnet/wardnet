import { screen, within } from "@testing-library/react";
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
  useRoutingProfiles,
  useCreateRoutingProfile,
  useUpdateRoutingProfile,
  useDeleteRoutingProfile,
  useDomainRoutingRules,
  navigate,
} = vi.hoisted(() => ({
  useRoutingProfiles: vi.fn(),
  useCreateRoutingProfile: vi.fn(),
  useUpdateRoutingProfile: vi.fn(),
  useDeleteRoutingProfile: vi.fn(),
  useDomainRoutingRules: vi.fn(),
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
    useRoutingProfiles,
    useCreateRoutingProfile,
    useUpdateRoutingProfile,
    useDeleteRoutingProfile,
    useDomainRoutingRules,
  };
});

import RoutingProfiles from "@/pages/RoutingProfiles";
import { renderWithProviders } from "../test-utils";

const createMutateAsync = vi.fn().mockResolvedValue({ profile: { id: "p9" } });
const renameMutateAsync = vi.fn().mockResolvedValue({ profile: { id: "p1" } });
const deleteMutateAsync = vi.fn().mockResolvedValue({ message: "gone" });

function mutations() {
  useCreateRoutingProfile.mockReturnValue({
    mutateAsync: createMutateAsync,
    isPending: false,
    error: null,
  });
  useUpdateRoutingProfile.mockReturnValue({
    mutateAsync: renameMutateAsync,
    isPending: false,
    error: null,
  });
  useDeleteRoutingProfile.mockReturnValue({
    mutateAsync: deleteMutateAsync,
    isPending: false,
    error: null,
  });
  useDomainRoutingRules.mockReturnValue({ data: { rules: [] } });
}

describe("RoutingProfiles", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mutations();
  });

  it("shows the empty state and creates a profile from the modal", async () => {
    const user = userEvent.setup();
    useRoutingProfiles.mockReturnValue({
      data: { profiles: [] },
      isLoading: false,
    });
    renderWithProviders(<RoutingProfiles />);

    expect(screen.getByText("No routing profiles")).toBeInTheDocument();

    await user.click(screen.getByTestId("routing-empty-add"));
    await user.type(screen.getByTestId("routing-profile-name"), "Streaming");
    await user.click(screen.getByTestId("routing-profile-name-save"));

    expect(createMutateAsync).toHaveBeenCalledWith({ name: "Streaming" });
  });

  it("lists profiles and navigates to the detail page on row click", async () => {
    const user = userEvent.setup();
    useRoutingProfiles.mockReturnValue({
      data: { profiles: [{ id: "p1", name: "Streaming" }] },
      isLoading: false,
    });
    renderWithProviders(<RoutingProfiles />);

    await user.click(screen.getByText("Streaming"));
    expect(navigate).toHaveBeenCalledWith("/routing/p1");
  });

  it("renames a profile through the modal", async () => {
    const user = userEvent.setup();
    useRoutingProfiles.mockReturnValue({
      data: { profiles: [{ id: "p1", name: "Streaming" }] },
      isLoading: false,
    });
    renderWithProviders(<RoutingProfiles />);

    await user.click(screen.getByTestId("routing-profile-rename"));
    const nameInput = screen.getByTestId("routing-profile-name");
    await user.clear(nameInput);
    await user.type(nameInput, "Streaming UK");
    await user.click(screen.getByTestId("routing-profile-name-save"));

    expect(renameMutateAsync).toHaveBeenCalledWith({
      id: "p1",
      body: { name: "Streaming UK" },
    });
  });

  it("deletes a profile after confirmation", async () => {
    const user = userEvent.setup();
    useRoutingProfiles.mockReturnValue({
      data: { profiles: [{ id: "p1", name: "Streaming" }] },
      isLoading: false,
    });
    renderWithProviders(<RoutingProfiles />);

    await user.click(screen.getByTestId("routing-profile-delete"));
    const dialog = await screen.findByRole("alertdialog");
    await user.click(within(dialog).getByTestId("confirm-dialog-confirm"));

    expect(deleteMutateAsync).toHaveBeenCalledWith("p1");
  });
});
