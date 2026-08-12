import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Anomaly } from "@wardnet/js";

const { useAnomalies } = vi.hoisted(() => ({
  useAnomalies: vi.fn((): { data: { anomalies: Anomaly[] } | undefined } => ({
    data: { anomalies: [] },
  })),
}));
vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, useAnomalies };
});

import { AnomaliesList } from "@/components/AnomaliesList";
import { renderWithProviders } from "../test-utils";

function anomaly(overrides: Partial<Anomaly> = {}): Anomaly {
  return {
    id: "a-1",
    type: "blocklist_refresh_failing",
    severity: "error",
    component: "dns",
    subject_id: "bl-1",
    message: "Blocklist HaGeZi has failed to refresh 7 times",
    hint: "check the URL is still reachable",
    details: { profile_id: "p-1" },
    opened_at: "2026-08-01T00:00:00Z",
    last_seen_at: "2026-08-01T06:00:00Z",
    occurrences: 7,
    resolved_at: null,
    ...overrides,
  };
}

describe("AnomaliesList", () => {
  // A permanently-present "nothing wrong" block costs more on a phone screen
  // than it says, so the section is absent when the box is healthy.
  it("renders nothing when there are no open anomalies", () => {
    useAnomalies.mockReturnValue({ data: { anomalies: [] } });
    renderWithProviders(<AnomaliesList />);
    expect(screen.queryByTestId("system-anomalies")).not.toBeInTheDocument();
  });

  it("renders nothing before the first response arrives", () => {
    useAnomalies.mockReturnValue({ data: undefined });
    renderWithProviders(<AnomaliesList />);
    expect(screen.queryByTestId("system-anomalies")).not.toBeInTheDocument();
  });

  it("shows the message and the remediation hint", () => {
    useAnomalies.mockReturnValue({ data: { anomalies: [anomaly()] } });
    renderWithProviders(<AnomaliesList />);

    expect(
      screen.getByText("Blocklist HaGeZi has failed to refresh 7 times"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("check the URL is still reachable"),
    ).toBeInTheDocument();
  });

  // This surface has no blocklist page, so the link resolves through the
  // route map's fallback to the section the anomaly belongs to.
  it("links a row to the section its component maps to", () => {
    useAnomalies.mockReturnValue({ data: { anomalies: [anomaly()] } });
    renderWithProviders(<AnomaliesList />);

    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "/dns");
    // Names the target: every row would otherwise share one accessible name.
    expect(link).toHaveAccessibleName(/Ad Blocking.*HaGeZi/i);
  });

  it("renders one row per open anomaly", () => {
    useAnomalies.mockReturnValue({
      data: {
        anomalies: [
          anomaly(),
          anomaly({
            id: "a-2",
            type: "tunnel_unhealthy",
            component: "tunnel",
            severity: "warning",
            subject_id: "tun-1",
            message: "Tunnel uk-1 is not responding",
          }),
        ],
      },
    });
    renderWithProviders(<AnomaliesList />);

    expect(screen.getAllByTestId("system-anomaly-row")).toHaveLength(2);
    expect(
      screen.getByText("Tunnel uk-1 is not responding"),
    ).toBeInTheDocument();
  });
});
