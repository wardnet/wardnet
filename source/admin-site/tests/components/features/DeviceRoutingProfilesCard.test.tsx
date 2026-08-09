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

import { DeviceRoutingProfilesCard } from "@/components/features/DeviceRoutingProfilesCard";
import { renderWithProviders, makeDevice } from "../../test-utils";
import type { RoutingProfile } from "@wardnet/js";

const profiles = [
  { id: "p1", name: "Streaming" },
  { id: "p2", name: "Work" },
  { id: "p3", name: "Kids" },
] as RoutingProfile[];

const saveMutateAsync = vi.fn();
const saveReset = vi.fn();

function setup(assigned: string[] = ["p1", "p2"]) {
  renderWithProviders(
    <DeviceRoutingProfilesCard
      device={makeDevice()}
      allProfiles={profiles}
      assignedIds={assigned}
      save={{
        mutateAsync: saveMutateAsync,
        reset: saveReset,
        isPending: false,
        isError: false,
        error: null,
      }}
    />,
  );
}

describe("DeviceRoutingProfilesCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    saveMutateAsync.mockResolvedValue({ message: "ok" });
  });

  it("shows the assigned profiles in priority order", () => {
    setup();
    expect(screen.getByText("Streaming")).toBeInTheDocument();
    expect(screen.getByText("Work")).toBeInTheDocument();
  });

  it("reorders with move-down then saves the whole ordered array", async () => {
    const user = userEvent.setup();
    setup(["p1", "p2"]);

    await user.click(screen.getByTestId("device-routing-profiles-edit"));

    // Move the first row (Streaming) down; order becomes [Work, Streaming].
    const moveDowns = screen.getAllByRole("button", { name: "Move down" });
    await user.click(moveDowns[0]);

    await user.click(screen.getByTestId("device-routing-profiles-save"));

    expect(saveMutateAsync).toHaveBeenCalledWith({
      deviceId: "dev-1",
      profileIds: ["p2", "p1"],
    });
  });

  it("removes a profile before saving", async () => {
    const user = userEvent.setup();
    setup(["p1", "p2"]);

    await user.click(screen.getByTestId("device-routing-profiles-edit"));
    const removeButtons = screen.getAllByRole("button", { name: "Remove" });
    await user.click(removeButtons[0]);
    await user.click(screen.getByTestId("device-routing-profiles-save"));

    expect(saveMutateAsync).toHaveBeenCalledWith({
      deviceId: "dev-1",
      profileIds: ["p2"],
    });
  });

  it("adds an unassigned profile from the picker", async () => {
    const user = userEvent.setup();
    setup(["p1"]);

    await user.click(screen.getByTestId("device-routing-profiles-edit"));
    await user.click(screen.getByTestId("device-routing-profile-add"));
    const listbox = await screen.findByRole("listbox");
    await user.click(within(listbox).getByText("Kids"));
    await user.click(screen.getByTestId("device-routing-profiles-save"));

    expect(saveMutateAsync).toHaveBeenCalledWith({
      deviceId: "dev-1",
      profileIds: ["p1", "p3"],
    });
  });

  it("reorders with move-up then saves", async () => {
    const user = userEvent.setup();
    setup(["p1", "p2"]);
    await user.click(screen.getByTestId("device-routing-profiles-edit"));
    // The first row's Move up is disabled; move the second row (Work) up.
    const moveUps = screen.getAllByRole("button", { name: "Move up" });
    await user.click(moveUps[1]);
    await user.click(screen.getByTestId("device-routing-profiles-save"));
    expect(saveMutateAsync).toHaveBeenCalledWith({
      deviceId: "dev-1",
      profileIds: ["p2", "p1"],
    });
  });

  it("cancels editing without saving", async () => {
    const user = userEvent.setup();
    setup(["p1", "p2"]);
    await user.click(screen.getByTestId("device-routing-profiles-edit"));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(saveMutateAsync).not.toHaveBeenCalled();
    expect(
      screen.getByTestId("device-routing-profiles-edit"),
    ).toBeInTheDocument();
  });
});
