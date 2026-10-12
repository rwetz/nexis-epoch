import * as React from "react";
import { baseOption, EChart, type EChartsOption, useChartTheme } from "./EChart";

/**
 * A labelled matrix of counts (a confusion matrix, a correlation table).
 * Sequential encoding: one hue, from the surface to series slot 1, so the
 * darkest cell is the largest count. Every cell is also labelled with its
 * number, so colour never carries the value alone.
 */
export function MatrixChart({
  rows,
  columns,
  values,
  rowName,
  columnName,
  className,
  label,
}: {
  rows: readonly string[];
  columns: readonly string[];
  values: readonly (readonly number[])[];
  rowName: string;
  columnName: string;
  className?: string;
  label: string;
}) {
  const t = useChartTheme();
  const option = React.useMemo<EChartsOption>(() => {
    const data: [number, number, number][] = [];
    let max = 0;
    values.forEach((row, r) =>
      row.forEach((v, c) => {
        data.push([c, r, v]);
        max = Math.max(max, v);
      }),
    );
    const showLabels = rows.length * columns.length <= 400;
    return {
      ...baseOption(t),
      grid: { left: 8, right: 8, top: 20, bottom: 28, containLabel: true },
      tooltip: {
        ...(baseOption(t).tooltip as object),
        formatter: (p: { value: [number, number, number] }) =>
          `${rowName} <b>${rows[p.value[1]]}</b> → ${columnName} <b>${columns[p.value[0]]}</b><br/>${p.value[2]}`,
      },
      xAxis: {
        type: "category",
        data: columns,
        name: columnName,
        nameLocation: "middle",
        nameGap: 22,
        nameTextStyle: { color: t.muted, fontSize: 10 },
        axisLabel: { color: t.muted, fontSize: 10 },
        axisLine: { show: false },
        axisTick: { show: false },
      },
      yAxis: {
        type: "category",
        data: rows,
        inverse: true,
        name: rowName,
        nameLocation: "end",
        nameTextStyle: { color: t.muted, fontSize: 10, align: "right" },
        axisLabel: { color: t.muted, fontSize: 10 },
        axisLine: { show: false },
        axisTick: { show: false },
      },
      visualMap: { show: false, min: 0, max: Math.max(1, max), inRange: { color: [t.surface, t.series[0]] } },
      series: [
        {
          type: "heatmap",
          data,
          label: { show: showLabels, color: t.text, fontSize: 10, fontFamily: t.fontMono },
          itemStyle: { borderColor: t.surface, borderWidth: 2, borderRadius: 3 },
          emphasis: { itemStyle: { borderColor: t.text, borderWidth: 1 } },
        },
      ],
    } satisfies EChartsOption;
  }, [rows, columns, values, rowName, columnName, t]);

  return <EChart option={option} label={label} className={className} />;
}
