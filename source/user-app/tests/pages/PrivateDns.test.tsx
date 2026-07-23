import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";

import PrivateDns from "../../src/pages/PrivateDns";
import { renderWithProviders } from "../test-utils";

const { usePrivateDnsMe } = vi.hoisted(() => ({
  usePrivateDnsMe: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, usePrivateDnsMe };
});

function setMe(data: {
  enabled: boolean;
  granted: boolean;
  hostname: string | null;
}) {
  usePrivateDnsMe.mockReturnValue({ data, isLoading: false });
}

describe("PrivateDns page", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows a loading state while me() resolves", () => {
    usePrivateDnsMe.mockReturnValue({ data: undefined, isLoading: true });
    renderWithProviders(<PrivateDns />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("tells the user to ask their admin when the feature is off", () => {
    setMe({ enabled: false, granted: false, hostname: null });
    renderWithProviders(<PrivateDns />);
    expect(
      screen.getByText(/isn't enabled on your network/),
    ).toBeInTheDocument();
  });

  it("prompts to be granted when enabled but not granted", () => {
    setMe({ enabled: true, granted: false, hostname: null });
    renderWithProviders(<PrivateDns />);
    expect(screen.getByText(/hasn't been granted/)).toBeInTheDocument();
  });

  it("shows the setup instructions with the hostname when granted", () => {
    setMe({
      enabled: true,
      granted: true,
      hostname: "tok.abc.my.wardnet.services",
    });
    renderWithProviders(<PrivateDns />);
    expect(screen.getByTestId("private-dns-hostname")).toHaveTextContent(
      "tok.abc.my.wardnet.services",
    );
  });
});
