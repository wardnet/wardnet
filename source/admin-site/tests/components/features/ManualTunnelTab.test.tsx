import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ManualTunnelTab } from "@/components/features/ManualTunnelTab";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useCreateTunnel: vi.fn() };
});

import { useCreateTunnel } from "@wardnet/web";

const mutateAsync = vi.fn();

function setHook(over: Record<string, unknown> = {}) {
  vi.mocked(useCreateTunnel).mockReturnValue({
    mutateAsync,
    isPending: false,
    isError: false,
    error: null,
    ...over,
  } as unknown as ReturnType<typeof useCreateTunnel>);
}

beforeEach(() => {
  vi.clearAllMocks();
  mutateAsync.mockResolvedValue(undefined);
  setHook();
});

describe("ManualTunnelTab", () => {
  it("submits with trimmed provider omitted and resets + calls onSuccess", async () => {
    const user = userEvent.setup();
    const onSuccess = vi.fn();
    renderWithProviders(<ManualTunnelTab onSuccess={onSuccess} />);

    await user.type(screen.getByLabelText("Label"), "US West");
    await user.type(screen.getByLabelText("Country code"), "US");
    await user.type(
      screen.getByLabelText("WireGuard config"),
      "PrivateKey=abc",
    );
    await user.click(screen.getByRole("button", { name: "Create tunnel" }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1));
    expect(mutateAsync).toHaveBeenCalledWith({
      label: "US West",
      country_code: "US",
      provider: undefined,
      config: "PrivateKey=abc",
    });
    expect(onSuccess).toHaveBeenCalled();
    // reset() clears the label field
    expect((screen.getByLabelText("Label") as HTMLInputElement).value).toBe("");
  });

  it("passes the provider through when filled", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ManualTunnelTab onSuccess={vi.fn()} />);

    await user.type(screen.getByLabelText("Label"), "L");
    await user.type(screen.getByLabelText("Country code"), "DE");
    await user.type(screen.getByLabelText("Provider"), "Mullvad");
    await user.type(screen.getByLabelText("WireGuard config"), "cfg");
    await user.click(screen.getByRole("button", { name: "Create tunnel" }));

    await waitFor(() => expect(mutateAsync).toHaveBeenCalled());
    expect(mutateAsync).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "Mullvad" }),
    );
  });

  it("blocks submit and shows validation for a non-2-letter country code", async () => {
    const user = userEvent.setup();
    renderWithProviders(<ManualTunnelTab onSuccess={vi.fn()} />);

    await user.type(screen.getByLabelText("Label"), "L");
    await user.type(screen.getByLabelText("Country code"), "U");
    await user.type(screen.getByLabelText("WireGuard config"), "cfg");
    await user.click(screen.getByRole("button", { name: "Create tunnel" }));

    expect(
      await screen.findByText("Must be a 2-letter ISO country code."),
    ).toBeInTheDocument();
    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it("shows a pending label and disables the button while creating", () => {
    setHook({ isPending: true });
    renderWithProviders(<ManualTunnelTab onSuccess={vi.fn()} />);
    const btn = screen.getByRole("button", { name: "Creating…" });
    expect(btn).toBeDisabled();
  });

  it("renders an error alert when the mutation errors", () => {
    setHook({ isError: true, error: new Error("boom") });
    renderWithProviders(<ManualTunnelTab onSuccess={vi.fn()} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
