import { useCallback, useState } from "react";

/**
 * Value window into the chart's XAxis domain.
 * `start` and `end` are the axis values at the edges of the selected region —
 * for timestamp charts these are millisecond numbers matching Recharts'
 * `activeLabel` so they can be used directly as XAxis `domain` props.
 */
export interface ZoomRange {
  start: number | string;
  end: number | string;
}

interface UseChartZoomOptions {
  /**
   * Identifier for the current dataset (range tab, data length, etc.).
   * When this changes any in-flight drag is discarded automatically.
   */
  datasetKey: string;
  /** Externally controlled zoom selection. `null` means full range. */
  zoom: ZoomRange | null;
  /** Notified once on mouse-up with the committed selection (or `null` on reset). */
  onZoomChange: (zoom: ZoomRange | null) => void;
}

// Recharts v3 MouseHandlerDataParam — only the field we need.
// activeTooltipIndex is `number | string | null` in v3 so we use
// activeLabel (the XAxis dataKey value) instead, matching the official
// HighlightAndZoomLineChart example.
interface RechartsMouseEvent {
  activeLabel?: number | string | null;
}

interface UseChartZoomResult {
  /** Recharts chart event props — spread onto AreaChart / LineChart. */
  chartProps: {
    onMouseDown: (e: RechartsMouseEvent | null) => void;
    onMouseMove: (e: RechartsMouseEvent | null) => void;
    onMouseUp: () => void;
    onMouseLeave: () => void;
  };
  /**
   * Live drag preview window — render as a `<ReferenceArea>` overlay.
   * `null` when the user is not currently dragging.
   */
  previewRange: ZoomRange | null;
  /** Whether a zoom selection is currently committed. */
  isZoomed: boolean;
  /** Clear the committed zoom. */
  reset: () => void;
}

/**
 * Drag-to-zoom behaviour for a Recharts time-series chart.
 *
 * Implements the pattern from the official Recharts HighlightAndZoomLineChart
 * example: onMouseDown anchors the drag from activeLabel, onMouseMove updates
 * the far edge, onMouseUp commits. ZoomRange stores axis values (timestamps)
 * so consumers pass them directly to XAxis `domain` and `ReferenceArea` x1/x2.
 */
export function useChartZoom({
  datasetKey,
  zoom,
  onZoomChange,
}: UseChartZoomOptions): UseChartZoomResult {
  // Drag state is tagged with the dataset key so a range-tab switch
  // automatically discards any in-flight drag without a useEffect reset.
  const [stored, setStored] = useState<{
    key: string;
    anchor: number | string;
    current: number | string;
  } | null>(null);
  const drag = stored?.key === datasetKey ? stored : null;

  const onMouseDown = useCallback(
    (e: RechartsMouseEvent | null) => {
      const label = e?.activeLabel;
      if (label == null) return;
      setStored({ key: datasetKey, anchor: label, current: label });
    },
    [datasetKey],
  );

  const onMouseMove = useCallback(
    (e: RechartsMouseEvent | null) => {
      const label = e?.activeLabel;
      if (label == null) return;
      setStored((prev) => {
        if (!prev || prev.key !== datasetKey) return prev;
        return { ...prev, current: label };
      });
    },
    [datasetKey],
  );

  const onMouseLeave = useCallback(() => {
    setStored(null);
  }, []);

  const onMouseUp = useCallback(() => {
    // Capture the zoom to commit before clearing drag state so we can call
    // onZoomChange outside the setState updater (calling external callbacks
    // inside updaters is unsafe in React 18 concurrent mode).
    let pending: ZoomRange | null = null;
    setStored((prev) => {
      if (!prev || prev.key !== datasetKey) return null;
      const { anchor, current } = prev;
      if (anchor === current) return null;
      const forward = Number(anchor) <= Number(current);
      pending = forward ? { start: anchor, end: current } : { start: current, end: anchor };
      return null;
    });
    if (pending) onZoomChange(pending);
  }, [datasetKey, onZoomChange]);

  const previewRange: ZoomRange | null =
    drag && drag.anchor !== drag.current
      ? Number(drag.anchor) <= Number(drag.current)
        ? { start: drag.anchor, end: drag.current }
        : { start: drag.current, end: drag.anchor }
      : null;

  const reset = useCallback(() => onZoomChange(null), [onZoomChange]);

  return {
    chartProps: { onMouseDown, onMouseMove, onMouseUp, onMouseLeave },
    previewRange,
    isZoomed: zoom != null,
    reset,
  };
}
