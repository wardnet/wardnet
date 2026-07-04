import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useAdvanceWizard,
  useDnsFilterProfiles,
  useDnsFilterConfig,
  useUpdateDnsFilterConfig,
} = vi.hoisted(() => ({
  useAdvanceWizard: vi.fn(),
  useDnsFilterProfiles: vi.fn(),
  useDnsFilterConfig: vi.fn(),
  useUpdateDnsFilterConfig: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useAdvanceWizard,
    useDnsFilterProfiles,
    useDnsFilterConfig,
    useUpdateDnsFilterConfig,
  };
});

import StepDns from "@/pages/setup/StepDns";
import { renderWithProviders } from "../../test-utils";

const advanceMutate = vi.fn();
const updateMutate = vi.fn();

const AD_BLOCKING = "00000000-0000-0000-0000-000000000100";
const PARENTAL = "00000000-0000-0000-0000-000000000101";
const MALWARE = "00000000-0000-0000-0000-000000000102";

const profiles = [
  {
    id: AD_BLOCKING,
    name: "Ad Blocking",
    description: "Blocks ads",
    builtin: true,
  },
  { id: PARENTAL, name: "Parental Controls", description: "", builtin: true },
  { id: MALWARE, name: "Malware & Phishing", description: "", builtin: true },
  { id: "custom-1", name: "My custom", description: "", builtin: false },
];

beforeEach(() => {
  vi.clearAllMocks();
  useAdvanceWizard.mockReturnValue({ mutate: advanceMutate, isPending: false });
  useDnsFilterProfiles.mockReturnValue({
    data: { profiles },
    isLoading: false,
  });
  useDnsFilterConfig.mockReturnValue({
    data: { config: { enabled: true, default_profile_ids: [AD_BLOCKING] } },
  });
  useUpdateDnsFilterConfig.mockReturnValue({
    mutate: updateMutate,
    isPending: false,
  });
});

describe("StepDns", () => {
  it("renders only the builtin profiles as toggle cards", () => {
    renderWithProviders(<StepDns />);
    expect(screen.getByText("Ad Blocking")).toBeInTheDocument();
    expect(screen.getByText("Parental Controls")).toBeInTheDocument();
    expect(screen.getByText("Malware & Phishing")).toBeInTheDocument();
    expect(screen.queryByText("My custom")).not.toBeInTheDocument();
  });

  it("requests silent config updates (no toast per toggle)", () => {
    renderWithProviders(<StepDns />);
    expect(useUpdateDnsFilterConfig).toHaveBeenCalledWith({ silent: true });
  });

  it("adds a profile to the default set when toggled on", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDns />);
    await user.click(screen.getByTestId(`setup-dns-profile-${MALWARE}`));
    expect(updateMutate).toHaveBeenCalledWith({
      default_profile_ids: [AD_BLOCKING, MALWARE],
    });
  });

  it("removes a profile from the default set when toggled off", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDns />);
    await user.click(screen.getByTestId(`setup-dns-profile-${AD_BLOCKING}`));
    expect(updateMutate).toHaveBeenCalledWith({ default_profile_ids: [] });
  });

  it("shows a loading state while profiles load", () => {
    useDnsFilterProfiles.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<StepDns />);
    expect(screen.getByText("Loading profiles…")).toBeInTheDocument();
    expect(screen.queryByText("Ad Blocking")).not.toBeInTheDocument();
  });

  it("disables toggles while a config update is in flight", () => {
    useUpdateDnsFilterConfig.mockReturnValue({
      mutate: updateMutate,
      isPending: true,
    });
    renderWithProviders(<StepDns />);
    expect(
      screen.getByTestId(`setup-dns-profile-${AD_BLOCKING}`),
    ).toBeDisabled();
  });

  it("advances to tunnel on continue", async () => {
    const user = userEvent.setup();
    renderWithProviders(<StepDns />);
    await user.click(screen.getByTestId("setup-dns-continue"));
    expect(advanceMutate).toHaveBeenCalledWith({ to_step: "tunnel" });
  });
});
