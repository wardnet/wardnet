import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTlsStatus } from "@wardnet/web";
import { DashboardRemoteAccessBanner } from "@/components/features/DashboardRemoteAccessBanner";
import { renderWithProviders } from "../../test-utils";

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useTlsStatus: vi.fn() };
});

const mockUseTlsStatus = vi.mocked(useTlsStatus);

function setStatus(status: unknown) {
  mockUseTlsStatus.mockReturnValue({
    data: status,
  } as unknown as ReturnType<typeof useTlsStatus>);
}

describe("DashboardRemoteAccessBanner", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders nothing when status is undefined", () => {
    setStatus(undefined);
    const { container } = renderWithProviders(<DashboardRemoteAccessBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing while idle", () => {
    setStatus({ phase: "idle" });
    const { container } = renderWithProviders(<DashboardRemoteAccessBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing once issued", () => {
    setStatus({ phase: "issued", domain: "home.example.com" });
    const { container } = renderWithProviders(<DashboardRemoteAccessBanner />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders the progress banner while issuing", () => {
    setStatus({ phase: "issuing", domain: "home.example.com" });
    renderWithProviders(<DashboardRemoteAccessBanner />);
    expect(screen.getByText("Issuing certificate…")).toBeInTheDocument();
  });

  it("renders the failure banner when failed", () => {
    setStatus({ phase: "failed", error: "boom" });
    renderWithProviders(<DashboardRemoteAccessBanner />);
    expect(screen.getByText("Certificate issuance failed")).toBeInTheDocument();
  });
});
