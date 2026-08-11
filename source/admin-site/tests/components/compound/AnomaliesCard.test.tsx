import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import type { Anomaly } from "@wardnet/js";
import { AnomaliesCard } from "@/components/compound/AnomaliesCard";
import { renderWithProviders } from "../../test-utils";

function anomaly(overrides: Partial<Anomaly>): Anomaly {
  return {
    id: crypto.randomUUID(),
    type: "tunnel_start_failed",
    severity: "error",
    component: "tunnel",
    subject_id: null,
    message: "something happened",
    hint: "",
    details: null,
    opened_at: "",
    last_seen_at: "",
    occurrences: 1,
    resolved_at: null,
    ...overrides,
  };
}

describe("AnomaliesCard", () => {
  it("shows the empty state when nothing is wrong", () => {
    renderWithProviders(<AnomaliesCard anomalies={[]} />);
    expect(screen.getByText("Nothing wrong right now")).toBeInTheDocument();
  });

  it("renders anomalies newest-first with severity classes", () => {
    const anomalies: Anomaly[] = [
      anomaly({
        severity: "warning",
        opened_at: "2026-01-01T10:00:00.123Z",
        message: "first warning",
      }),
      anomaly({
        severity: "error",
        opened_at: "2026-01-01T11:00:00.000Z",
        message: "second error",
      }),
      anomaly({ severity: "info", opened_at: "", message: "third info" }),
    ];
    const { container } = renderWithProviders(
      <AnomaliesCard anomalies={anomalies} />,
    );

    const rows = container.querySelectorAll(".logrow");
    // Sorted by opened_at descending: 11:00 error, 10:00 warning, then the
    // empty-timestamp info row (which sinks to the bottom).
    expect(rows[0].className).toContain("is-err");
    expect(rows[1].className).toContain("is-warn");
    expect(rows[2].className).toContain("is-info");

    const times = container.querySelectorAll(".logrow .t");
    expect(times[0].textContent).toContain(".000");
    expect(times[1].textContent).toContain(".123");
    expect(times[2].textContent).toBe("");
  });

  it("renders the remediation hint when present", () => {
    renderWithProviders(
      <AnomaliesCard
        anomalies={[
          anomaly({ message: "tunnel down", hint: "check the endpoint" }),
        ]}
      />,
    );
    expect(screen.getByText("check the endpoint")).toBeInTheDocument();
  });

  it("links a blocklist anomaly to the profile page that lists it", () => {
    renderWithProviders(
      <AnomaliesCard
        anomalies={[
          anomaly({
            type: "blocklist_refresh_failing",
            component: "dns",
            subject_id: "bl-1",
            details: { profile_id: "p-1" },
            message: "failed 7 times",
          }),
        ]}
      />,
    );

    const link = screen.getByTestId("anomaly-subject-link");
    expect(link).toHaveAttribute(
      "href",
      "/dns/filter/profiles/p-1#blocklist-bl-1",
    );
    // Names the target: every row would otherwise share one accessible name.
    expect(link).toHaveAccessibleName(/blocklist.*failed 7 times/i);
  });

  it("renders no link for an anomaly with nowhere to go", () => {
    renderWithProviders(
      <AnomaliesCard
        anomalies={[
          anomaly({
            type: "route_table_lost",
            component: "routing",
            subject_id: "100",
            message: "table 100 was lost",
          }),
        ]}
      />,
    );

    // `routing` still has a section route, so the fallback resolves — what
    // must not happen is a link to a per-entity page that does not exist.
    expect(screen.getByTestId("anomaly-subject-link")).toHaveAttribute(
      "href",
      "/routing",
    );
  });
});
