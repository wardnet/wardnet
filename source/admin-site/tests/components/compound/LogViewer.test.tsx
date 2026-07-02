import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LogEntry } from "@wardnet/js";
import { LogViewer } from "@/components/compound/LogViewer";
import { renderWithProviders } from "../../test-utils";

function entry(overrides: Partial<LogEntry> = {}): LogEntry {
  return {
    timestamp: "2026-01-01T10:20:30.123Z",
    level: "INFO",
    target: "wardnet::mod",
    message: "hello",
    ...overrides,
  };
}

describe("LogViewer", () => {
  it("shows the waiting message when connected with no entries", () => {
    renderWithProviders(<LogViewer entries={[]} connected skipped={0} />);
    expect(screen.getByText("Waiting for log entries…")).toBeInTheDocument();
  });

  it("shows the not-connected message when disconnected with no entries", () => {
    renderWithProviders(
      <LogViewer entries={[]} connected={false} skipped={0} />,
    );
    expect(screen.getByText("Not connected")).toBeInTheDocument();
  });

  it("renders a buffer-lag warning when entries were skipped", () => {
    renderWithProviders(
      <LogViewer entries={[]} connected skipped={7} maxHeight="10rem" />,
    );
    expect(
      screen.getByText("7 entries skipped (buffer lag)"),
    ).toBeInTheDocument();
  });

  it("renders entries with level modifiers and formatted messages", () => {
    renderWithProviders(
      <LogViewer
        entries={[
          entry({
            level: "ERROR",
            message: "boom",
            fields: { user: "alice", message: "ignored" },
            span: {
              method: "GET",
              path: "/api/x",
              status: "200",
              latency_ms: "5",
              name: "handler",
              trace_id: "abc",
            },
          }),
          entry({ level: "WARN", message: "response", timestamp: "" }),
          entry({ level: "DEBUG", message: "debug line", target: "t" }),
        ]}
        connected
        skipped={0}
      />,
    );
    // ERROR row: message + HTTP context + dedup'd fields.
    expect(
      screen.getByText(/boom GET \/api\/x 200 \(5ms\)/),
    ).toBeInTheDocument();
    expect(screen.getByText(/user=alice/)).toBeInTheDocument();
    expect(screen.getByText(/trace_id=abc/)).toBeInTheDocument();
    // Generic "response" message with no span falls back to the raw message.
    expect(screen.getAllByText("response").length).toBeGreaterThan(0);
    // DEBUG maps to the info modifier and shows the message.
    expect(screen.getByText("debug line")).toBeInTheDocument();
    // Level labels are upper-cased.
    expect(screen.getByText("ERROR")).toBeInTheDocument();
    expect(screen.getByText("WARN")).toBeInTheDocument();
    expect(screen.getByText("DEBUG")).toBeInTheDocument();
  });
});
