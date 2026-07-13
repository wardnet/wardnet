import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dnsFilterService } = vi.hoisted(() => ({
  dnsFilterService: { getConfig: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dnsFilterService, jobsService: {} }));

import { DnsFilterNotReadyNotice } from "../../src/components/DnsFilterNotReadyNotice";

const w = createQueryWrapper;

/** The whole point of this notice: filtering fails open on startup and after the
 *  blocklist-cache reset, and the admin must not have to guess that. */
describe("DnsFilterNotReadyNotice", () => {
  beforeEach(() => vi.clearAllMocks());

  it("warns while enabled blocklists are still downloading", async () => {
    dnsFilterService.getConfig.mockResolvedValue({
      config: { enabled: true, default_profile_ids: [] },
      filters_ready: false,
      pending_blocklists: 2,
    });

    render(<DnsFilterNotReadyNotice />, { wrapper: w() });

    expect(
      await screen.findByTestId("dns-filter-not-ready-notice"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/2 enabled blocklists are still downloading/i),
    ).toBeInTheDocument();
  });

  it("stays silent once filtering is actually enforced", async () => {
    dnsFilterService.getConfig.mockResolvedValue({
      config: { enabled: true, default_profile_ids: [] },
      filters_ready: true,
      pending_blocklists: 0,
    });

    const { container } = render(<DnsFilterNotReadyNotice />, { wrapper: w() });
    await vi.waitFor(() =>
      expect(dnsFilterService.getConfig).toHaveBeenCalled(),
    );

    expect(
      screen.queryByTestId("dns-filter-not-ready-notice"),
    ).not.toBeInTheDocument();
    expect(container).toBeEmptyDOMElement();
  });

  it("stays silent when the admin has turned filtering off entirely", async () => {
    // Nothing is *meant* to be enforced, so a fail-open warning would be noise.
    dnsFilterService.getConfig.mockResolvedValue({
      config: { enabled: false, default_profile_ids: [] },
      filters_ready: false,
      pending_blocklists: 3,
    });

    render(<DnsFilterNotReadyNotice />, { wrapper: w() });
    await vi.waitFor(() =>
      expect(dnsFilterService.getConfig).toHaveBeenCalled(),
    );

    expect(
      screen.queryByTestId("dns-filter-not-ready-notice"),
    ).not.toBeInTheDocument();
  });
});
