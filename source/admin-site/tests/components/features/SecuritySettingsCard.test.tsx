import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDnsConfig, useUpdateDnsConfig } from "@wardnet/web";
import { SecuritySettingsCard } from "@/components/features/SecuritySettingsCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsConfig: vi.fn(),
    useUpdateDnsConfig: vi.fn(),
  };
});

vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast: { success: vi.fn(), error: vi.fn() } };
});

const mockConfig = vi.mocked(useDnsConfig);
const mockUpdate = vi.mocked(useUpdateDnsConfig);
const mutate = vi.fn();

function setConfig(
  config: Record<string, unknown> | undefined,
  isLoading = false,
) {
  mockConfig.mockReturnValue({
    data: config ? { config } : undefined,
    isLoading,
  } as unknown as ReturnType<typeof useDnsConfig>);
}

describe("SecuritySettingsCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockUpdate.mockReturnValue({
      mutate,
      isPending: false,
    } as unknown as ReturnType<typeof useUpdateDnsConfig>);
  });

  it("reflects the loaded config in the toggles", () => {
    setConfig({
      dnssec_enabled: true,
      rebinding_protection: false,
      rate_limit_per_second: 50,
    });
    renderWithProviders(<SecuritySettingsCard />);

    expect(screen.getByLabelText("Enable DNSSEC validation")).toBeChecked();
    expect(
      screen.getByLabelText("Enable DNS rebinding protection"),
    ).not.toBeChecked();
    expect(screen.getByRole("spinbutton")).toHaveValue(50);
  });

  it("mutates when DNSSEC and rebinding toggles change", async () => {
    setConfig({ dnssec_enabled: false, rebinding_protection: true });
    renderWithProviders(<SecuritySettingsCard />);

    await userEvent.click(screen.getByLabelText("Enable DNSSEC validation"));
    expect(mutate).toHaveBeenCalledWith({ dnssec_enabled: true });

    await userEvent.click(
      screen.getByLabelText("Enable DNS rebinding protection"),
    );
    expect(mutate).toHaveBeenCalledWith({ rebinding_protection: false });
  });

  it("shows Save when the rate limit is dirty and saves the parsed value", async () => {
    setConfig({ rate_limit_per_second: 0 });
    renderWithProviders(<SecuritySettingsCard />);

    // No Save button until the rate is edited to a new value.
    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "25");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(mutate).toHaveBeenCalledWith(
      { rate_limit_per_second: 25 },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    );
  });

  it("shows a validation error and disables Save for a bad value", async () => {
    setConfig({ rate_limit_per_second: 0 });
    renderWithProviders(<SecuritySettingsCard />);

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "-5");

    expect(screen.getByText("Enter a whole number ≥ 0.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("cancel resets the rate edit buffer", async () => {
    setConfig({ rate_limit_per_second: 10 });
    renderWithProviders(<SecuritySettingsCard />);

    const input = screen.getByRole("spinbutton");
    await userEvent.clear(input);
    await userEvent.type(input, "99");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    // Reverts to the config value and the actions row disappears.
    expect(screen.getByRole("spinbutton")).toHaveValue(10);
    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();
  });
});
