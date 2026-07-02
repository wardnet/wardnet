import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDnsFilterConfig, useUpdateDnsFilterConfig } from "@wardnet/web";
import { DnsFilterSettingsCard } from "@/components/features/DnsFilterSettingsCard";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsFilterConfig: vi.fn(),
    useUpdateDnsFilterConfig: vi.fn(),
  };
});

const mockConfig = vi.mocked(useDnsFilterConfig);
const mockUpdate = vi.mocked(useUpdateDnsFilterConfig);
const mutate = vi.fn();

function setUpdate(over: Record<string, unknown> = {}) {
  mockUpdate.mockReturnValue({
    mutate,
    isPending: false,
    ...over,
  } as unknown as ReturnType<typeof useUpdateDnsFilterConfig>);
}

describe("DnsFilterSettingsCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setUpdate();
  });

  it("shows Enabled and a checked toggle when filtering is on", () => {
    mockConfig.mockReturnValue({
      data: { config: { enabled: true } },
      isLoading: false,
    } as unknown as ReturnType<typeof useDnsFilterConfig>);

    renderWithProviders(<DnsFilterSettingsCard />);

    expect(screen.getByText("Enabled")).toBeInTheDocument();
    expect(screen.getByLabelText("Enable DNS filtering")).toBeChecked();
  });

  it("shows Disabled when filtering is off and toggles it on", async () => {
    mockConfig.mockReturnValue({
      data: { config: { enabled: false } },
      isLoading: false,
    } as unknown as ReturnType<typeof useDnsFilterConfig>);

    renderWithProviders(<DnsFilterSettingsCard />);

    expect(screen.getByText("Disabled")).toBeInTheDocument();
    const toggle = screen.getByLabelText("Enable DNS filtering");
    expect(toggle).not.toBeChecked();

    await userEvent.click(toggle);
    expect(mutate).toHaveBeenCalledWith({ enabled: true });
  });

  it("disables the toggle while loading", () => {
    mockConfig.mockReturnValue({
      data: undefined,
      isLoading: true,
    } as unknown as ReturnType<typeof useDnsFilterConfig>);

    renderWithProviders(<DnsFilterSettingsCard />);

    expect(screen.getByLabelText("Enable DNS filtering")).toBeDisabled();
  });
});
