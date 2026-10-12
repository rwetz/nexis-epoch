import * as React from "react";
import { cn, Icon, Input, Switch } from "@nexis/design";
import type { ScalarValue, TomlNode } from "@/types/backend";

/**
 * The structured view of a TOML document: tables, arrays and values, with
 * scalars editable in place. Edits go to the backend's format-preserving
 * writer, which returns new source text; this component never rewrites the
 * file itself.
 */
export function TomlInspector({
  tree,
  editable,
  onEdit,
  onReveal,
}: {
  tree: readonly TomlNode[];
  editable: boolean;
  onEdit(node: TomlNode, value: ScalarValue): void;
  onReveal(line: number): void;
}) {
  const [collapsed, setCollapsed] = React.useState<Set<string>>(new Set());
  const toggle = (key: string) =>
    setCollapsed((c) => {
      const n = new Set(c);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });

  const render = (nodes: readonly TomlNode[], depth: number): React.ReactNode =>
    nodes.map((node) => {
      const key = JSON.stringify(node.path);
      const container = node.kind !== "scalar";
      const open = !collapsed.has(key);
      return (
        <React.Fragment key={key}>
          <div
            className="group flex min-h-7 items-center gap-1.5 rounded-md pr-1 text-[12px] hover:bg-muted/50"
            style={{ paddingLeft: 6 + depth * 12 }}
          >
            {container ? (
              <button
                type="button"
                aria-expanded={open}
                aria-label={open ? `Collapse ${node.label}` : `Expand ${node.label}`}
                onClick={() => toggle(key)}
                className="grid size-4 place-items-center text-muted-foreground"
              >
                <Icon name={open ? "chevron-down" : "chevron-right"} size={11} />
              </button>
            ) : (
              <span className="size-4" />
            )}
            <button
              type="button"
              title={node.line ? `Line ${node.line}` : undefined}
              onClick={() => node.line && onReveal(node.line)}
              className={cn(
                "min-w-0 shrink truncate text-left font-mono",
                container ? "font-medium text-foreground" : "text-muted-foreground",
              )}
            >
              {node.kind === "table" || node.kind === "array_of_tables" ? `[${node.label}]` : node.label}
            </button>
            {container ? (
              <span className="ml-auto shrink-0 font-mono text-[10px] text-muted-foreground/70">
                {node.kind === "array" || node.kind === "array_of_tables" ? `${node.children.length} items` : `${node.children.length} keys`}
              </span>
            ) : (
              node.value && <ValueEditor value={node.value} disabled={!editable} onCommit={(v) => onEdit(node, v)} />
            )}
          </div>
          {container && open && render(node.children, depth + 1)}
        </React.Fragment>
      );
    });

  return <div role="tree" aria-label="Configuration structure" className="flex flex-col py-1">{render(tree, 0)}</div>;
}

function ValueEditor({ value, disabled, onCommit }: { value: ScalarValue; disabled: boolean; onCommit(v: ScalarValue): void }) {
  const shown = value.type === "float" && value.value === null ? "nan" : String(value.value);
  const [draft, setDraft] = React.useState(shown);
  const [invalid, setInvalid] = React.useState(false);
  React.useEffect(() => {
    setDraft(shown);
    setInvalid(false);
  }, [shown]);

  if (value.type === "boolean") {
    return (
      <Switch
        size="sm"
        className="ml-auto"
        checked={value.value}
        disabled={disabled}
        onCheckedChange={(v) => onCommit({ type: "boolean", value: v })}
        aria-label="Toggle value"
      />
    );
  }

  const commit = () => {
    if (draft === shown) return;
    let next: ScalarValue | null = null;
    if (value.type === "integer") {
      if (/^[+-]?\d+$/.test(draft.trim())) next = { type: "integer", value: Number(draft) };
      else if (/^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(draft.trim())) next = { type: "float", value: Number(draft) };
    } else if (value.type === "float") {
      const n = Number(draft);
      if (draft.trim() !== "" && Number.isFinite(n)) next = { type: "float", value: n };
    } else if (value.type === "string") {
      next = { type: "string", value: draft };
    } else {
      next = { type: "datetime", value: draft.trim() };
    }
    if (next) onCommit(next);
    else setInvalid(true);
  };

  return (
    <Input
      value={draft}
      disabled={disabled}
      aria-invalid={invalid}
      aria-label="Value"
      onChange={(e) => {
        setDraft(e.target.value);
        setInvalid(false);
      }}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setDraft(shown);
      }}
      className={cn(
        "ml-auto h-6 w-36 shrink-0 px-1.5 font-mono text-[11.5px]",
        value.type !== "string" && "text-right tabular-nums",
        invalid && "border-destructive",
      )}
    />
  );
}
