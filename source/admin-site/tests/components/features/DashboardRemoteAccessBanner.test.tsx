import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DashboardRemoteAccessBanner } from "@/components/features/DashboardRemoteAccessBanner";
import { renderWithProviders } from "../../test-utils";
import type { TlsStatusResponse } from "@wardnet/js";

function renderBanner(status: unknown) {
  return renderWithProviders(
    <DashboardRemoteAccessBanner
      status={status as TlsStatusResponse | undefined}
    />,
  );
}

describe("DashboardRemoteAccessBanner", () => {
  it("renders nothing when status is undefined", () => {
    const { container } = renderBanner(undefined);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing while idle", () => {
    const { container } = renderBanner({ phase: "idle" });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing once issued", () => {
    const { container } = renderBanner({
      phase: "issued",
      domain: "home.example.com",
    });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders the progress banner while issuing", () => {
    renderBanner({ phase: "issuing", domain: "home.example.com" });
    expect(screen.getByText("Issuing certificate…")).toBeInTheDocument();
  });

  it("renders the failure banner when failed", () => {
    renderBanner({ phase: "failed", error: "boom" });
    expect(screen.getByText("Certificate issuance failed")).toBeInTheDocument();
  });
});
