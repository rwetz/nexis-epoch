/**
 * A weights file (`.safetensors`, `.onnx`) described from its header:
 * parameter count, dtypes, the ONNX graph's interface and operators, and
 * every tensor with its shape. Nothing is loaded or run (epoch-core
 * `model_file`).
 */
import * as React from "react";
import { Badge, PropertyList, SectionLabel, Stat } from "@nexis/design";
import { ErrorState, LoadingState, useAsync } from "@epoch/components/states";
import { api } from "@epoch/lib/api";
import { formatBytes, formatCount } from "@epoch/lib/format";
import type { ModelFileInfo, TensorInfo } from "@epoch/types/backend";

/** 1.2M, 840K, 7.3B: the way parameter counts are spoken about. */
export function formatParams(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(n >= 1e10 ? 0 : 1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`;
  if (n >= 1e4) return `${(n / 1e3).toFixed(0)}K`;
  return formatCount(n);
}

const shapeText = (t: TensorInfo) => (t.shape.length ? `[${t.shape.join(", ")}]` : "scalar");

function Interface({ title, tensors }: { title: string; tensors: TensorInfo[] }) {
  if (tensors.length === 0) return null;
  return (
    <div className="flex flex-col gap-1">
      <SectionLabel>{title}</SectionLabel>
      <PropertyList items={tensors.map((t) => ({ label: t.name, value: `${t.dtype} ${shapeText(t)}`, mono: true }))} />
    </div>
  );
}

function TensorTable({ info }: { info: ModelFileInfo }) {
  const [filter, setFilter] = React.useState("");
  const rows = React.useMemo(() => {
    const q = filter.trim().toLowerCase();
    return q ? info.tensors.filter((t) => t.name.toLowerCase().includes(q)) : info.tensors;
  }, [info.tensors, filter]);
  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <SectionLabel className="flex-1">
          {info.format === "onnx" ? "Initializers" : "Tensors"} · {formatCount(info.tensorCount)}
        </SectionLabel>
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="Filter by name"
          aria-label="Filter tensors by name"
          className="h-6 w-40 rounded-md border border-border bg-transparent px-2 text-[11px] outline-none focus:border-ring"
        />
      </div>
      <div className="overflow-hidden rounded-lg border border-border/60">
        <table className="w-full border-collapse text-[11px]">
          <thead className="bg-muted/40 text-left text-muted-foreground">
            <tr>
              <th className="px-2 py-1 font-medium">Name</th>
              <th className="px-2 py-1 font-medium">Type</th>
              <th className="px-2 py-1 font-medium">Shape</th>
              <th className="px-2 py-1 text-right font-medium">Parameters</th>
            </tr>
          </thead>
          <tbody className="font-mono tabular-nums">
            {rows.map((t, i) => (
              <tr key={`${t.name}-${i}`} className="border-t border-border/40 [content-visibility:auto]">
                <td className="max-w-0 truncate px-2 py-1" title={t.name}>
                  {t.name || <span className="text-muted-foreground">unnamed</span>}
                </td>
                <td className="px-2 py-1 text-muted-foreground">{t.dtype}</td>
                <td className="px-2 py-1">{shapeText(t)}</td>
                <td className="px-2 py-1 text-right">{t.elements === null ? "—" : formatCount(t.elements)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {rows.length === 0 && <p className="px-2 py-3 text-center text-[11px] text-muted-foreground">No tensor matches.</p>}
      </div>
      {info.truncated && (
        <p className="px-1 text-[11px] text-muted-foreground">
          The first {formatCount(info.tensors.length)} are listed; the totals above count all {formatCount(info.tensorCount)}.
        </p>
      )}
    </div>
  );
}

export function ModelFileView({ path }: { path: string }) {
  const { state, reload } = useAsync(() => api.modelInspect(path), [path]);
  if (state.status === "error") return <ErrorState error={state.error} onRetry={reload} title="Couldn't read this model file" />;
  if (state.status !== "ready") return <LoadingState label="Reading header…" />;
  const info = state.data;
  const onnx = info.onnx;
  const metadata = Object.entries(info.metadata);
  return (
    <div className="nexis-content-enter nexis-scrollbar flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-2">
        <Stat label="Parameters" value={formatParams(info.parameters)} caption={formatCount(info.parameters)} />
        <Stat label={info.format === "onnx" ? "Initializers" : "Tensors"} value={formatCount(info.tensorCount)} />
        <Stat label="File" value={formatBytes(info.size)} caption={info.format === "onnx" ? "ONNX" : "Safetensors"} />
        {onnx && <Stat label="Operators" value={formatCount(onnx.nodeCount)} caption={`${onnx.ops.length} kinds`} />}
      </div>
      {info.dtypes.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          {info.dtypes.map(([dtype, n]) => (
            <Badge key={dtype} variant="outline" className="font-mono text-[10.5px]">
              {dtype} · {formatParams(n)}
            </Badge>
          ))}
        </div>
      )}
      {onnx && (
        <>
          <div className="flex flex-col gap-1">
            <SectionLabel>Graph</SectionLabel>
            <PropertyList
              items={[
                { label: "Name", value: onnx.graphName || "—", mono: true },
                { label: "Producer", value: onnx.producer || "not recorded", mono: true },
                { label: "IR version", value: onnx.irVersion ?? "—", mono: true },
                { label: "Opsets", value: onnx.opsets.map(([d, v]) => `${d} ${v}`).join(", ") || "—", mono: true },
              ]}
            />
          </div>
          <Interface title="Inputs" tensors={onnx.inputs} />
          <Interface title="Outputs" tensors={onnx.outputs} />
          {onnx.ops.length > 0 && (
            <div className="flex flex-col gap-1.5">
              <SectionLabel>Operators</SectionLabel>
              <div className="flex flex-wrap gap-1.5">
                {onnx.ops.map((o) => (
                  <span key={o.op} className="rounded-md bg-muted/60 px-1.5 py-0.5 font-mono text-[10.5px]">
                    {o.op} <span className="text-muted-foreground">×{o.count}</span>
                  </span>
                ))}
              </div>
            </div>
          )}
        </>
      )}
      {metadata.length > 0 && (
        <div className="flex flex-col gap-1">
          <SectionLabel>Metadata</SectionLabel>
          <PropertyList items={metadata.map(([k, v]) => ({ label: k, value: v, mono: true }))} />
        </div>
      )}
      <TensorTable info={info} />
    </div>
  );
}
