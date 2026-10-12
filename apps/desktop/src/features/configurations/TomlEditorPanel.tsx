import * as React from "react";
import { Badge, Button, cn, Icon, toast } from "@nexis/design";
import { confirmAction } from "@/components/confirm";
import { CodeEditor, type CodeEditorHandle } from "@/components/editor/CodeEditor";
import { PanelToolbar, ToolbarButton, ToolbarTitle } from "@/components/PanelToolbar";
import { EmptyState, ErrorState, LoadingState } from "@/components/states";
import { useApp } from "@/app/store";
import { usePanel } from "@/features/workspace/panel-context";
import { definePanel, stringParams } from "@/features/workspace/registry";
import { api } from "@/lib/api";
import { basename } from "@/lib/fileIcons";
import { BackendError, errorMessage } from "@/lib/ipc";
import type { ParseReport, ScalarValue, TomlNode } from "@/types/backend";
import { TomlInspector } from "./TomlInspector";

type Loaded = { savedText: string; hash: string };

function TomlEditor() {
  const panel = usePanel("toml-editor");
  const path = panel.params.path;
  if (!path) return <ConfigPicker onPick={(p) => panel.setParams({ path: p })} />;
  return <TomlFile key={path} path={path} />;
}

function ConfigPicker({ onPick }: { onPick(path: string): void }) {
  const project = useApp((s) => s.project);
  if (!project) return <EmptyState icon="settings" title="No project open" description="Open a project to edit its configuration." />;
  const files = [...new Set([...project.trainConfigs, ...(project.hasPyproject ? ["pyproject.toml"] : [])])];
  return (
    <EmptyState
      icon="settings"
      title="Open a TOML file"
      description={files.length ? "Configuration files found in this project:" : "No train.toml or pyproject.toml found. Open any .toml from the project explorer."}
      action={
        files.length > 0 && (
          <div className="flex w-full max-w-72 flex-col gap-1">
            {files.slice(0, 12).map((f) => (
              <Button key={f} size="sm" variant="ghost" className="justify-start font-mono text-xs" onClick={() => onPick(f)}>
                <Icon name="settings" /> <span className="truncate">{f}</span>
              </Button>
            ))}
          </div>
        )
      }
    />
  );
}

function TomlFile({ path }: { path: string }) {
  const panel = usePanel("toml-editor");
  const editor = React.useRef<CodeEditorHandle>(null);
  const [loaded, setLoaded] = React.useState<Loaded | null>(null);
  const [loadError, setLoadError] = React.useState<unknown>(null);
  const [text, setText] = React.useState("");
  const [report, setReport] = React.useState<ParseReport | null>(null);
  // The last tree that parsed, shown (read-only) while the text is broken.
  const [lastTree, setLastTree] = React.useState<TomlNode[]>([]);
  const [saving, setSaving] = React.useState(false);
  const [showInspector, setShowInspector] = React.useState(true);

  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const file = await api.readText(path);
      setLoaded({ savedText: file.content, hash: file.hash });
      setText(file.content);
    } catch (e) {
      setLoadError(e);
    }
  }, [path]);
  React.useEffect(() => void load(), [load]);

  // Validate as the user types (the parse is cheap and pure).
  React.useEffect(() => {
    if (!loaded) return;
    const id = setTimeout(() => {
      api.tomlParse(text).then((r) => {
        setReport(r);
        if (r.ok) setLastTree(r.tree);
      }, () => {});
    }, 150);
    return () => clearTimeout(id);
  }, [text, loaded]);

  const dirty = loaded !== null && text !== loaded.savedText;
  React.useEffect(() => panel.setDirty(dirty), [dirty]); // eslint-disable-line react-hooks/exhaustive-deps
  const dirtyRef = React.useRef(dirty);
  dirtyRef.current = dirty;
  React.useEffect(() => {
    panel.setCloseGuard(async () =>
      !dirtyRef.current ||
      confirmAction({ title: `Discard changes to ${basename(path)}?`, description: "Your edits have not been saved.", confirmLabel: "Discard", destructive: true }),
    );
    return () => panel.setCloseGuard(null);
  }, [path]); // eslint-disable-line react-hooks/exhaustive-deps

  const save = React.useCallback(
    async (force = false) => {
      if (!loaded || saving) return;
      setSaving(true);
      try {
        const file = await api.writeText(path, text, force ? null : loaded.hash);
        setLoaded({ savedText: file.content, hash: file.hash });
        toast.success(`Saved ${basename(path)}`);
      } catch (e) {
        if (e instanceof BackendError && e.code === "conflict") {
          const overwrite = await confirmAction({
            title: `${basename(path)} changed on disk`,
            description: "Another program modified the file after you opened it. Overwrite it with your version, or cancel and reload to see theirs.",
            confirmLabel: "Overwrite",
            destructive: true,
          });
          if (overwrite) {
            setSaving(false);
            return save(true);
          }
        } else {
          toast.error("Couldn't save", { description: errorMessage(e) });
        }
      } finally {
        setSaving(false);
      }
    },
    [loaded, saving, path, text],
  );

  const reload = async () => {
    if (dirty && !(await confirmAction({ title: "Reload from disk?", description: "Your unsaved edits will be lost.", confirmLabel: "Reload", destructive: true }))) return;
    await load();
  };

  const edit = async (node: TomlNode, value: ScalarValue) => {
    try {
      setText(await api.tomlSetValue(text, node.path, value));
    } catch (e) {
      toast.error("Couldn't change the value", { description: errorMessage(e) });
    }
  };

  if (loadError) return <ErrorState error={loadError} onRetry={() => void load()} />;
  if (!loaded) return <LoadingState label={`Opening ${basename(path)}…`} />;

  const errors = report?.diagnostics.length ?? 0;
  return (
    <>
      <PanelToolbar>
        <ToolbarTitle icon="settings">
          <span title={path}>{path}</span>
        </ToolbarTitle>
        {report && (
          <Badge variant={report.ok ? "success" : "destructive"} className="h-5 shrink-0 gap-1 px-1.5 text-[10.5px]">
            <Icon name={report.ok ? "check" : "alert-circle"} size={10} />
            {report.ok ? "Valid TOML" : `${errors} error${errors === 1 ? "" : "s"}`}
          </Badge>
        )}
        <span className="flex-1" />
        <ToolbarButton icon="refresh" label="Reload from disk" onClick={() => void reload()} />
        <ToolbarButton icon="outline" label="Structure" active={showInspector} onClick={() => setShowInspector((s) => !s)} />
        <Button size="xs" variant={dirty ? "brand" : "ghost"} disabled={!dirty || saving} onClick={() => void save()}>
          <Icon name="save" /> {saving ? "Saving…" : "Save"}
        </Button>
      </PanelToolbar>
      <div className="flex min-h-0 flex-1">
        <div className="flex min-w-0 flex-1 flex-col">
          <CodeEditor
            ref={editor}
            label={`Editing ${path}`}
            language="toml"
            value={text}
            onChange={setText}
            onSave={() => void save()}
            diagnostics={report?.diagnostics}
          />
          {report && !report.ok && (
            <button
              type="button"
              onClick={() => editor.current?.goToLine(report.diagnostics[0].line)}
              className="flex shrink-0 items-center gap-2 border-t border-border/60 bg-destructive/8 px-3 py-1.5 text-left text-[11.5px] text-destructive"
            >
              <Icon name="alert-circle" size="xs" />
              <span className="font-mono">
                {report.diagnostics[0].line}:{report.diagnostics[0].column}
              </span>
              <span className="truncate">{report.diagnostics[0].message}</span>
            </button>
          )}
        </div>
        {showInspector && (
          <aside className="flex w-72 shrink-0 flex-col border-l border-border/60 bg-sidebar/30">
            <div className="flex h-8 shrink-0 items-center justify-between px-3 text-[10.5px] font-medium tracking-wide text-muted-foreground uppercase">
              Structure
              {report && !report.ok && <span className="normal-case tracking-normal">read-only until fixed</span>}
            </div>
            <div className={cn("nexis-scrollbar min-h-0 flex-1 overflow-y-auto px-1", report && !report.ok && "opacity-60")}>
              {lastTree.length === 0 ? (
                <p className="px-3 py-4 text-xs text-muted-foreground">{report?.ok ? "The document is empty." : "Waiting for a valid document."}</p>
              ) : (
                <TomlInspector tree={lastTree} editable={!!report?.ok} onEdit={(n, v) => void edit(n, v)} onReveal={(l) => editor.current?.goToLine(l)} />
              )}
            </div>
            <p className="shrink-0 border-t border-border/60 px-3 py-2 text-[10.5px] leading-relaxed text-muted-foreground">
              Value edits keep comments and layout. The edited value itself is re-written in standard form (e.g. <span className="font-mono">1_000</span> → <span className="font-mono">1000</span>).
            </p>
          </aside>
        )}
      </div>
    </>
  );
}

export const tomlEditorPanel = definePanel({
  type: "toml-editor",
  title: "Configuration",
  icon: "settings",
  description: "Edit TOML with validation, a structured view and format-preserving value edits.",
  component: TomlEditor,
  parseParams: (raw) => stringParams(raw, ["path"] as const),
  tabTitle: (p) => (p.path ? basename(p.path) : null),
  placement: "stage",
  maturity: "partial",
  limitations: "Schema-driven forms for known configs (train.toml) are planned; today values are edited generically.",
});
