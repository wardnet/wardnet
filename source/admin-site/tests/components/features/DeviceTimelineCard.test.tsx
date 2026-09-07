import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeviceTimelineCard } from "@/components/features/DeviceTimelineCard";
import { renderWithProviders } from "../../test-utils";
import type { DeviceTimelineResponse } from "@wardnet/js";

const { useDeviceTimeline } = vi.hoisted(() => ({
  useDeviceTimeline: vi.fn(),
}));

vi.mock("@wardnet/web", async () => {
  const actual = await vi.importActual<Record<string, unknown>>("@wardnet/web");
  return { ...actual, useDeviceTimeline };
});

function makeTimeline(
  over: Partial<DeviceTimelineResponse> = {},
): DeviceTimelineResponse {
  return {
    from: "2026-09-06T00:00:00Z",
    to: "2026-09-07T00:00:00Z",
    bucket_secs: 3600,
    events: [],
    dhcp: [],
    dns: [],
    ...over,
  };
}

function renderCard(data: DeviceTimelineResponse | undefined, state = {}) {
  useDeviceTimeline.mockReturnValue({
    data,
    isLoading: false,
    isError: false,
    ...state,
  });
  return renderWithProviders(<DeviceTimelineCard deviceId="device-1" />);
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("DeviceTimelineCard", () => {
  it("defaults to the 24 hour window", () => {
    renderCard(makeTimeline());

    expect(useDeviceTimeline).toHaveBeenCalledWith(
      "device-1",
      "twenty_four_hours",
    );
    expect(screen.getByRole("button", { name: "24h" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("refetches for the window the admin picks", async () => {
    renderCard(makeTimeline());

    await userEvent.click(screen.getByRole("button", { name: "1h" }));

    expect(useDeviceTimeline).toHaveBeenLastCalledWith("device-1", "one_hour");
  });

  /** The split by result is what rules DNS in or out; a total cannot. */
  it("shows DNS activity broken down by result", () => {
    renderCard(
      makeTimeline({
        dns: [
          {
            at: "2026-09-06T10:00:00Z",
            results: { forwarded: 40, blocked: 5 },
          },
          { at: "2026-09-06T11:00:00Z", results: { forwarded: 12 } },
        ],
      }),
    );

    expect(screen.getByText("forwarded")).toBeInTheDocument();
    expect(screen.getByText("52")).toBeInTheDocument();
    expect(screen.getByText("blocked")).toBeInTheDocument();
    expect(screen.getByText("5")).toBeInTheDocument();
  });

  it("renders observations and DHCP events on one list, newest first", () => {
    renderCard(
      makeTimeline({
        events: [
          {
            at: "2026-09-06T10:00:00Z",
            kind: "ip_changed",
            details: { old_ip: "192.168.100.23", new_ip: "192.168.100.41" },
          },
        ],
        dhcp: [
          {
            at: "2026-09-06T12:00:00Z",
            event_type: "renewed",
            details: null,
          },
        ],
      }),
    );

    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Lease renewed");
    expect(items[1]).toHaveTextContent("Changed address");
    expect(items[1]).toHaveTextContent("192.168.100.23 → 192.168.100.41");
  });

  /** "It never departed" is the observation that rules association out, so a
   *  departure has to be visually distinct from routine churn. */
  it("marks a departure as noteworthy", () => {
    renderCard(
      makeTimeline({
        events: [
          { at: "2026-09-06T10:00:00Z", kind: "gone", details: null },
        ],
      }),
    );

    expect(screen.getByText("Went away")).toBeInTheDocument();
  });

  it("explains an empty window rather than showing a blank card", () => {
    renderCard(makeTimeline());

    expect(screen.getByText(/Nothing recorded in this window/)).toBeInTheDocument();
    expect(screen.getByText(/No DNS queries in this window/)).toBeInTheDocument();
  });

  it("reports a failure to load", () => {
    renderCard(undefined, { isError: true });

    expect(
      screen.getByText(/Could not load this device's activity/),
    ).toBeInTheDocument();
  });
});
