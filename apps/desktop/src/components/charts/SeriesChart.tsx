import * as React from "react";
import { formatMetric } from "@/lib/format";
import { baseOption, EChart, type EChartsOption, useChartTheme } from "./EChart";

export type XYSeries = {
  /** Shown in the legend and tooltip. */
  label: string;
  points: readonly (readonly [number, number])[];
  /** Index into the categorical slots; follows the entity, not its rank. */
  slot: number;
};

/**
 * One measure over a shared x axis, for one or more series — loss per step
 * for several runs, say. One y axis only: a different measure is a
 * different chart.
 */
export function SeriesChart({
  title,
  series,
  xLabel = "step",
  className,
}: {
  title: string;
  series: readonly XYSeries[];
  xLabel?: string;
  className?: string;
}) {
  const t = useChartTheme();
  const option = React.useMemo<EChartsOption>(() => {
    const many = series.length > 1;
    return {
      ...baseOption(t),
      grid: { left: 8, right: many && series.length <= 4 ? 64 : 16, top: many ? 30 : 12, bottom: 26, containLabel: true },
      legend: many
        ? { top: 0, left: 0, itemWidth: 14, itemHeight: 2, icon: "rect", textStyle: { color: t.muted, fontSize: 11 } }
        : undefined,
      tooltip: {
        ...(baseOption(t).tooltip as object),
        trigger: "axis",
        axisPointer: { type: "cross", lineStyle: { color: t.muted }, crossStyle: { color: t.muted }, label: { show: false } },
        valueFormatter: (v: unknown) => (typeof v === "number" ? formatMetric(v) : String(v)),
      },
      xAxis: {
        type: "value",
        name: xLabel,
        nameLocation: "end",
        nameGap: 6,
        nameTextStyle: { color: t.muted, fontSize: 10, align: "right", verticalAlign: "top", padding: [8, 0, 0, 0] },
        scale: true,
        axisLine: { lineStyle: { color: t.axis } },
        axisTick: { show: false },
        axisLabel: { color: t.muted, fontSize: 10, hideOverlap: true },
        splitLine: { show: false },
      },
      yAxis: {
        type: "value",
        scale: true,
        axisLine: { show: false },
        axisTick: { show: false },
        axisLabel: { color: t.muted, fontSize: 10, formatter: (v: number) => formatMetric(v) },
        splitLine: { lineStyle: { color: t.grid } },
      },
      dataZoom: [{ type: "inside", xAxisIndex: 0, filterMode: "none" }],
      series: series.map((s) => {
        const color = t.series[s.slot % t.series.length];
        return {
          type: "line",
          name: s.label,
          data: s.points,
          showSymbol: false,
          symbolSize: 8,
          lineStyle: { width: 2, color },
          itemStyle: { color, borderColor: t.surface, borderWidth: 2 },
          emphasis: { focus: "series" },
          // Direct labels at the line end when there are few lines.
          endLabel:
            many && series.length <= 4
              ? { show: true, color: t.muted, fontSize: 10, formatter: (p: { value: [number, number] }) => formatMetric(p.value[1]) }
              : undefined,
          sampling: "lttb",
        };
      }),
    } satisfies EChartsOption;
  }, [series, t, xLabel]);

  return <EChart option={option} label={`${title} chart`} className={className} />;
}
