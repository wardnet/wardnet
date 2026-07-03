import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TlsStatusResponse } from "@wardnet/js";
import { RemoteAccessProgress } from "@/components/features/RemoteAccessProgress";
import { renderWithProviders } from "../../test-utils";

function status(over: Partial<TlsStatusResponse>): TlsStatusResponse {
  return {
    phase: "idle",
    domain: null,
    not_after: null,
    error: null,
    ...over,
  } as TlsStatusResponse;
}

describe("RemoteAccessProgress", () => {
  it("renders nothing for the idle phase", () => {
    const { container } = renderWithProviders(
      <RemoteAccessProgress status={status({ phase: "idle" })} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("shows the domain while issuing", () => {
    renderWithProviders(
      <RemoteAccessProgress
        status={status({ phase: "issuing", domain: "home.example.com" })}
      />,
    );
    expect(screen.getByText("Issuing certificate…")).toBeInTheDocument();
    expect(screen.getByText("home.example.com")).toBeInTheDocument();
  });

  it("shows generic copy while issuing with no domain", () => {
    renderWithProviders(
      <RemoteAccessProgress status={status({ phase: "issuing" })} />,
    );
    expect(
      screen.getByText("Provisioning HTTPS. This can take a minute."),
    ).toBeInTheDocument();
  });

  it("renders the issued state with expiry", () => {
    renderWithProviders(
      <RemoteAccessProgress
        status={status({
          phase: "issued",
          domain: "home.example.com",
          not_after: "2027-01-01T00:00:00Z",
        })}
      />,
    );
    expect(screen.getByText("Remote access is live")).toBeInTheDocument();
    expect(screen.getByText("home.example.com")).toBeInTheDocument();
  });

  it("renders the failure state with the daemon error", () => {
    renderWithProviders(
      <RemoteAccessProgress
        status={status({ phase: "failed", error: "DNS challenge failed" })}
      />,
    );
    expect(screen.getByText("Certificate issuance failed")).toBeInTheDocument();
    expect(screen.getByText(/DNS challenge failed/)).toBeInTheDocument();
  });

  it("renders a fallback failure message when error is null", () => {
    renderWithProviders(
      <RemoteAccessProgress status={status({ phase: "failed" })} />,
    );
    expect(
      screen.getByText(/The daemon could not issue a certificate\./),
    ).toBeInTheDocument();
  });
});
