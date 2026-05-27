# Wardnet Domain Glossary

## Chart infrastructure (admin UI)

**StatsRange**
The unified time-window discriminant used across every stats chart and data-fetching hook: `"1h" | "12h" | "24h" | "7d" | "12mo"`. Defined in `hooks/useStats.ts` alongside the companion `RANGE_HOURS` map (range → numeric hours). All chart components accept `StatsRange`; the legacy per-hook range aliases have been removed.

**RANGE_HOURS**
A `Record<StatsRange, number>` that maps each range string to its equivalent in hours. Used by chart `XAxis` tick formatters to select the appropriate date/time representation: HH:MM for ≤24 h, "Jan 5 14:00" for ≤168 h, and "Jan 5" for longer ranges.

**ZoomRange**
An inclusive index window into a sorted dataset: `{ startIndex: number; endIndex: number }`. Both indices refer to positions in the *unzoomed* data array. Exported from `hooks/useChartZoom.ts`.

**useChartZoom**
React hook that owns drag-to-zoom behaviour for a Recharts time-series chart. Accepts `{ length, datasetKey, zoom, onZoomChange }` and returns `{ chartProps, previewRange, isZoomed, reset }`.

- `chartProps` — Recharts mouse-event handlers (`onMouseDown / onMouseMove / onMouseUp / onMouseLeave`); spread directly onto the Recharts root element (`<AreaChart>`, `<LineChart>`, etc.).
- `previewRange` — live `ZoomRange | null` during an active drag; render as a `<ReferenceArea>` so the user sees the pending window.
- `isZoomed` — `true` when a committed zoom selection is narrower than the full dataset.
- `reset` — clears the committed zoom (calls `onZoomChange(null)`).

**datasetKey**
A string that uniquely identifies the current dataset snapshot, e.g. `"24h|143"` (range + point count). Passed to `useChartZoom`; when it changes, any in-flight drag is automatically discarded without an extra `useEffect`. Prevents stale indices from a previous data fetch being committed as a zoom selection.

**ZoomableChartContainer**
A compound component at `components/compound/ZoomableChartContainer.tsx` that wraps shadcn's `ChartContainer` and renders an overlaid "Reset zoom" ghost button when `isZoomed` is true. The chart's Recharts event wiring (`chartProps`) stays on the caller — `ZoomableChartContainer` only handles the visual container and reset affordance. Replaces bare `<ChartContainer>` on all interactive stats charts.

**applyZoom**
Pure helper exported from `hooks/useChartZoom.ts`. Slices a data array down to the `ZoomRange` window (`data.slice(start, end + 1)`). Keep calls inside a `useMemo` alongside other per-chart derived values.

**Lifted zoom state**
When two or more charts on the same page share a time window (e.g. tunnel throughput + latency on `TunnelDetail`), the `zoom: ZoomRange | null` state is lifted to the parent and passed down as props. Each chart receives the same `zoom` and `onZoomChange`; they share one `useChartZoom` call or each call `useChartZoom` with the same `zoom`/`onZoomChange`. Single-chart sections (e.g. `DnsStatsSection`) own their zoom locally.

**useTunnelStats**
Hook at `hooks/useTunnelStats.ts`. Fetches `tunnel.bytes.tx`, `tunnel.bytes.rx`, and `tunnel.latency.rtt_ms` for a single tunnel via `StatsService.queryMulti` with `label_filter = {"tunnel_id":"<uuid>"}`. Returns `TunnelStatsData { points: TunnelStatsPoint[]; bucketSecs: number; range: StatsRange }`. The parent (`TunnelDetail`) makes a single call and fans the result out to both `TunnelThroughputChart` and `TunnelLatencyChart`.

**TunnelStatsData / TunnelStatsPoint**
The shape returned by `useTunnelStats`. Each point carries `{ ts: string; bytesTx: number; bytesRx: number; rttMs: number | null }`. `rttMs` is nullable because the latency probe may not have run yet for a given bucket.

**label_filter (StatsQuery)**
An exact-match filter on the `labels` JSON column in `stats_intraday` / `stats_hourly` / `stats_daily`. Passed as a JSON string (e.g. `'{"tunnel_id":"<uuid>"}'`) to scope a query to a single resource. No partial/single-key match — the full sorted-keys JSON object must match.
