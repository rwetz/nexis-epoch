import * as React from "react";
import { toast } from "@nexis/design";
import { LoadingState } from "@/components/states";
import { useApp } from "@/app/store";
import { getWorkspace } from "./presets";
import { loadLayout, saveLayout } from "./persistence";
import { presetDocument, repairDocument, TrellisWorkspace } from "./trellis/TrellisWorkspace";
import type { WorkspaceApi, WorkspaceId } from "./types";
import { useWorkspacePrefs } from "./prefs";

const SAVE_DELAY_MS = 400;

/**
 * One workspace: loads its saved layout (migrating and repairing it), mounts
 * the layout adapter, and saves changes after a short pause.
 */
export function WorkspaceHost({ id, active }: { id: WorkspaceId; active: boolean }) {
  const preset = getWorkspace(id);
  const register = useApp((s) => s.registerWorkspace);
  const navigation = useWorkspacePrefs((s) => s.navigation);
  const [initial, setInitial] = React.useState<ReturnType<typeof presetDocument> | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    void loadLayout(id).then(({ document, notice }) => {
      if (cancelled) return;
      if (notice) toast(`${preset.name} layout`, { description: notice });
      setInitial(repairDocument(document) ?? presetDocument(preset));
    });
    return () => {
      cancelled = true;
    };
  }, [id, preset]);

  const timer = React.useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const latest = React.useRef<unknown>(null);
  const flush = React.useCallback(() => {
    clearTimeout(timer.current);
    if (latest.current === null) return;
    const doc = latest.current;
    latest.current = null;
    saveLayout(id, preset.version, doc).catch((e: unknown) =>
      toast.error("Couldn't save the layout", { description: e instanceof Error ? e.message : String(e) }),
    );
  }, [id, preset.version]);
  const onChange = React.useCallback(
    (doc: unknown) => {
      latest.current = doc;
      clearTimeout(timer.current);
      timer.current = setTimeout(flush, SAVE_DELAY_MS);
    },
    [flush],
  );
  React.useEffect(() => {
    window.addEventListener("beforeunload", flush);
    return () => {
      window.removeEventListener("beforeunload", flush);
      flush();
    };
  }, [flush]);

  const onReady = React.useCallback((api: WorkspaceApi | null) => register(api, id), [register, id]);

  return (
    <div
      className="epoch-workspace absolute inset-0 p-1.5"
      data-active={active}
      aria-hidden={!active}
      inert={!active}
    >
      {initial ? (
        <TrellisWorkspace
          preset={preset}
          initialDocument={initial}
          onDocumentChange={onChange}
          onReady={onReady}
          navigation={navigation}
        />
      ) : (
        <LoadingState label={`Restoring ${preset.name}…`} />
      )}
    </div>
  );
}
