/**
 * ECharts in a panel: resizes with its container (workspace panels resize
 * constantly), re-themes when the palette changes, and disposes on unmount.
 * Charts take generic options built from generic data; nothing here knows
 * about losses, accuracies or architectures.
 */
import * as React from "react";
import { BarChart, HeatmapChart, LineChart } from "echarts/charts";
import {
  DataZoomComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
} from "echarts/components";
import * as echarts from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { cn, resolveCssColor, useTheme } from "@nexis/design";

echarts.use([
  LineChart,
  BarChart,
  HeatmapChart,
  GridComponent,
  TooltipComponent,
  LegendComponent,
  DataZoomComponent,
  VisualMapComponent,
  CanvasRenderer,
]);

export type EChartsOption = echarts.EChartsCoreOption;

export const SERIES_SLOTS = 5;

export type ChartTheme = {
  text: string;
  muted: string;
  grid: string;
  axis: string;
  surface: string;
  tooltip: string;
  tooltipBorder: string;
  /** Categorical series colours, fixed order (styles.css --series-*). */
  series: string[];
  fontSans: string;
  fontMono: string;
};

/** Theme colours resolved for canvas, refreshed when the palette changes. */
export function useChartTheme(): ChartTheme {
  const { paletteEpoch, resolvedMode } = useTheme();
  return React.useMemo(() => {
    const css = getComputedStyle(document.documentElement);
    const c = (v: string) => resolveCssColor(`var(--${v})`);
    return {
      text: c("foreground"),
      muted: c("muted-foreground"),
      grid: resolveCssColor("color-mix(in oklab, var(--border) 70%, transparent)"),
      axis: c("border"),
      surface: c("card"),
      tooltip: c("popover"),
      tooltipBorder: c("border"),
      series: Array.from({ length: SERIES_SLOTS }, (_, i) => c(`series-${i + 1}`)),
      fontSans: css.getPropertyValue("--font-sans").trim() || "sans-serif",
      fontMono: css.getPropertyValue("--font-mono").trim() || "monospace",
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paletteEpoch, resolvedMode]);
}

export function EChart({
  option,
  className,
  label,
}: {
  option: EChartsOption;
  className?: string;
  /** Accessible name for the chart region. */
  label: string;
}) {
  const host = React.useRef<HTMLDivElement>(null);
  const surface = React.useRef<HTMLDivElement>(null);
  const chart = React.useRef<echarts.ECharts | null>(null);

  React.useEffect(() => {
    if (!host.current || !surface.current) return;
    const instance = echarts.init(surface.current, undefined, { renderer: "canvas" });
    chart.current = instance;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const el = host.current;
        if (el && el.clientWidth > 0 && el.clientHeight > 0) instance.resize();
      });
    });
    observer.observe(host.current);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      instance.dispose();
      chart.current = null;
    };
  }, []);

  React.useEffect(() => {
    // Live data redraws in place; no entry animation (house rule: motion
    // marks events, data never animates).
    chart.current?.setOption({ animation: false, ...option }, { notMerge: true });
  }, [option]);

  // ECharts pins its canvas to fixed pixel sizes. Drawn straight into the
  // host, that canvas is the host's content: until the observer's next frame
  // resizes it, a shrunk host has the old, larger canvas spilling over its
  // neighbours, and it props up the min-content size of whatever holds it.
  // The absolute, clipped layer keeps the canvas out of layout entirely.
  return (
    <div ref={host} role="img" aria-label={label} className={cn("relative min-h-0 min-w-0 overflow-hidden", className)}>
      <div ref={surface} className="absolute inset-0" />
    </div>
  );
}

/** Shared axis/tooltip styling for every chart. */
export function baseOption(t: ChartTheme): EChartsOption {
  return {
    textStyle: { fontFamily: t.fontSans, color: t.text, fontSize: 11 },
    tooltip: {
      backgroundColor: t.tooltip,
      borderColor: t.tooltipBorder,
      textStyle: { color: t.text, fontSize: 11, fontFamily: t.fontSans },
      extraCssText: "border-radius:8px;box-shadow:0 8px 24px -8px rgba(0,0,0,.35);",
    },
  };
}
