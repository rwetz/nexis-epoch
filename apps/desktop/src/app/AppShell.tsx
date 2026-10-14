import { useNexisLinks } from "@/features/links/links";
import * as React from "react";
import { CommandPalette, MOD_PROP, useCommandPaletteShortcut, WindowResizeEdges } from "@nexis/design";
import { ConfirmHost } from "@/components/confirm";
import { chooseAndOpenProject, bootstrap } from "@/features/projects/actions";
import { WorkspaceHost } from "@/features/workspace/WorkspaceHost";
import { useCommands, useFileCommands } from "./commands";
import { OverviewPage } from "./pages/OverviewPage";
import { PipelinesPage } from "./pages/PipelinesPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { Sidebar } from "./Sidebar";
import { StatusFacts } from "./StatusFacts";
import { useApp } from "./store";
import { TopBar } from "./TopBar";

type PaletteMode = "commands" | "search";

/** Mod+<key> (without Shift), in the capture phase so editors don't eat it. */
function useModKey(key: string, run: () => void) {
  const ref = React.useRef(run);
  ref.current = run;
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = MOD_PROP === "meta" ? e.metaKey : e.ctrlKey;
      if (mod && !e.shiftKey && !e.altKey && e.key.toLowerCase() === key) {
        e.preventDefault();
        ref.current();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [key]);
}

export function AppShell() {
  const location = useApp((s) => s.location);
  const mounted = useApp((s) => s.mounted);
  const goTo = useApp((s) => s.goTo);
  const toggleSidebar = useApp((s) => s.toggleSidebar);
  const [palette, setPalette] = React.useState<PaletteMode | null>(null);

  React.useEffect(() => void bootstrap(), []);
  // Links from Nexis: open a project, a run or a dataset (features/links).
  useNexisLinks();

  useCommandPaletteShortcut(() => setPalette((p) => (p === "commands" ? null : "commands")));
  useModKey("p", () => setPalette((p) => (p === "search" ? null : "search")));
  useModKey("o", () => void chooseAndOpenProject());
  useModKey("b", toggleSidebar);
  useModKey(",", () => goTo({ kind: "page", page: "settings" }));

  const commands = useCommands();
  const files = useFileCommands(palette === "search");
  const paletteCommands = React.useMemo(() => (palette === "search" ? [...files, ...commands] : commands), [palette, files, commands]);

  const activeWorkspace = location.kind === "workspace" ? location.workspace : null;

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <WindowResizeEdges />
      <TopBar onPalette={() => setPalette("commands")} onSearch={() => setPalette("search")} />
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="relative min-w-0 flex-1">
          {/* Workspaces stay mounted once opened, so panels keep their state
              while the user visits a page or another workspace. */}
          {mounted.map((id) => (
            <WorkspaceHost key={id} id={id} active={id === activeWorkspace} />
          ))}
          {location.kind === "page" && (
            <div className="absolute inset-0 bg-background">
              {location.page === "overview" && <OverviewPage />}
              {location.page === "projects" && <ProjectsPage />}
              {location.page === "pipelines" && <PipelinesPage />}
              {location.page === "settings" && <SettingsPage />}
            </div>
          )}
        </main>
      </div>
      <StatusFacts />
      <CommandPalette
        open={palette !== null}
        onOpenChange={(open) => !open && setPalette(null)}
        commands={paletteCommands}
        placeholder={palette === "search" ? "Search files and commands…" : "Type a command…"}
      />
      <ConfirmHost />
    </div>
  );
}
