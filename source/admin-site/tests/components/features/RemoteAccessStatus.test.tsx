import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type {
  DdnsResolutionCheckResponse,
  DdnsStatusResponse,
  TlsStatusResponse,
} from "@wardnet/js";
import { RemoteAccessStatus } from "@/components/features/RemoteAccessStatus";
import { renderWithProviders } from "../../test-utils";

function tls(over: Partial<TlsStatusResponse> = {}): TlsStatusResponse {
  return {
    phase: "idle",
    domain: null,
    not_after: null,
    error: null,
    ...over,
  } as TlsStatusResponse;
}

describe("RemoteAccessStatus", () => {
  it("banner variant only renders the progress view", () => {
    renderWithProviders(
      <RemoteAccessStatus tls={tls({ phase: "issuing" })} variant="banner" />,
    );
    expect(screen.getByText("Issuing certificate…")).toBeInTheDocument();
    expect(screen.queryByText("Public hostname")).not.toBeInTheDocument();
  });

  it("full variant renders the detail grid with fallbacks", () => {
    renderWithProviders(
      <RemoteAccessStatus tls={tls({ phase: "idle" })} variant="full" />,
    );
    expect(screen.getByText("Public hostname")).toBeInTheDocument();
    expect(screen.getByText("Not issued yet")).toBeInTheDocument();
    // Provider label fallback and resolution fallback both render em dashes.
    expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(2);
  });

  it("renders wardnet provider label and cert expiry", () => {
    const ddns = {
      fqdn: "home.example.com",
      provider: "wardnet",
    } as DdnsStatusResponse;
    renderWithProviders(
      <RemoteAccessStatus
        tls={tls({ phase: "issued", not_after: "2027-01-01T00:00:00Z" })}
        variant="full"
        ddns={ddns}
      />,
    );
    expect(screen.getByText("home.example.com")).toBeInTheDocument();
    expect(screen.getByText("Wardnet")).toBeInTheDocument();
    expect(screen.getByText(/Valid until/)).toBeInTheDocument();
  });

  it("renders cloudflare provider label", () => {
    const ddns = {
      fqdn: "home.example.com",
      provider: "cloudflare",
    } as DdnsStatusResponse;
    renderWithProviders(
      <RemoteAccessStatus tls={tls()} variant="full" ddns={ddns} />,
    );
    expect(screen.getByText("Your domain (Cloudflare)")).toBeInTheDocument();
  });

  it("renders a match verdict with resolved IPs and checked time", () => {
    const resolution = {
      verdict: "match",
      resolved_ips: ["1.2.3.4"],
    } as DdnsResolutionCheckResponse;
    renderWithProviders(
      <RemoteAccessStatus
        tls={tls()}
        variant="full"
        resolution={resolution}
        lastCheckedAt={Date.now()}
      />,
    );
    expect(screen.getByText("Resolves correctly")).toBeInTheDocument();
    expect(screen.getByText("1.2.3.4")).toBeInTheDocument();
    expect(screen.getByText(/Checked/)).toBeInTheDocument();
  });

  it("renders each verdict label", () => {
    const cases: Array<[string, string]> = [
      ["mismatch", "Points to the wrong IP"],
      ["pending", "Not yet visible publicly"],
      ["not_configured", "Not configured"],
    ];
    for (const [verdict, label] of cases) {
      const { unmount } = renderWithProviders(
        <RemoteAccessStatus
          tls={tls()}
          variant="full"
          resolution={
            {
              verdict,
              resolved_ips: [],
            } as unknown as DdnsResolutionCheckResponse
          }
        />,
      );
      expect(screen.getByText(label)).toBeInTheDocument();
      unmount();
    }
  });
});
