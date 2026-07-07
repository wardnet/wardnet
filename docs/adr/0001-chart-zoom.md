# ADR: Drop Recharts Brush in favour of drag-to-zoom

**Status**: Accepted  
**Date**: 2026-05-27  
**Issue**: #317

---

## Context

Stats charts in the admin UI previously used the Recharts `<Brush>` component — a mini-map scrubber rendered below the plot area — to let users navigate time windows. Several problems emerged as the chart surface grew:

1. **Layout friction.** Brush adds a fixed-height row below the plot, consuming vertical space and visually separating the scrubber from the data it controls.
2. **Shared-state mismatch.** The tunnel detail page needs two charts (throughput and latency) to share a single zoom window. Brush is owned by the chart it lives in; lifting its state requires wrapping it in a controlled form that fights the Recharts API.
3. **Numeric XAxis incompatibility.** Constraining the XAxis domain to a zoom window requires `type="number"` with explicit `domain` props. Brush's index-based selection doesn't compose cleanly with numeric domains without extra conversion.
4. **Touch/drag ambiguity.** Brush scrubbing conflicts with native scroll on touch devices.

---

## Decision

Replace `<Brush>` with a drag-to-zoom pattern built on Recharts mouse events and a `<ReferenceArea>` preview overlay:

- **`useChartZoom`** (`hooks/useChartZoom.ts`) — hook that tracks `mouseDown`/`mouseMove`/`mouseUp` on the Recharts root. Returns `chartProps` (event handlers to spread on the chart), `previewRange` (live `ZoomRange | null` for the drag preview), `isZoomed`, and `reset`. Zoom state is caller-owned (`zoom: ZoomRange | null`) so sibling charts on the same page can share one window.
- **`ZoomableChartContainer`** (`components/compound/ZoomableChartContainer.tsx`) — wraps shadcn's `<ChartContainer>` and renders a "Reset zoom" ghost button overlay when `isZoomed` is true. The chart's event wiring stays on the consumer.
- **`ZoomRange`** (`{ startIndex, endIndex }`) — an inclusive index pair into the unzoomed data array. XAxis domain is derived by reading `data[zoom.startIndex].tsMs` / `data[zoom.endIndex].tsMs`.

All interactive stats charts (DNS queries over time, tunnel throughput, tunnel latency) now use this pattern. No `<Brush>` component remains in the codebase.

---

## Zoom state ownership

| Chart(s) | Where `zoom` state lives | Reason |
|---|---|---|
| `TunnelThroughputChart` + `TunnelLatencyChart` | `TunnelDetail` (parent) | Both charts share one window; lifted state keeps them in sync |
| DNS queries chart in `DnsStatsSection` | `DnsStatsSection` (local) | Single chart; no sibling to share with |

If a future page needs more than two charts sharing a window, the pattern is the same — lift `zoom` and `range` to the nearest common ancestor and pass them down.

---

## Consequences

- Charts using this pattern require `type="number"` on `XAxis` with `dataKey="tsMs"` (millisecond timestamps). The data array must have a `tsMs` field (`new Date(p.ts).getTime()`).
- `datasetKey` (e.g. `"24h|143"`) must be passed to `useChartZoom` so stale drag indices from a previous fetch are discarded automatically when the data changes.
- `zoom` must be reset to `null` whenever the user changes the range tab (prevents a stale index window from being applied to a differently-sized dataset).
- `applyZoom(data, zoom)` is available as a pure helper for consumers that need the sliced array (e.g. for window-total calculations). Keep calls in `useMemo`.
- Single-point mouse clicks (start === end) are treated as plain chart interactions, not zoom selections, and are discarded by the hook.
