import { Button, cn, Icon, SidebarNav, type SidebarNavSection } from "@nexis/design";
import type { WorkspaceId } from "@/features/workspace/types";
import { useExperiments } from "@/features/experiments/store";
import { type Location, type PageId, useApp } from "./store";

type NavId = PageId | `ws:${WorkspaceId}`;

function toId(l: Location): NavId {
  return l.kind === "page" ? l.page : `ws:${l.workspace}`;
}

function toLocation(id: NavId): Location {
  return id.startsWith("ws:") ? { kind: "workspace", workspace: id.slice(3) as WorkspaceId } : { kind: "page", page: id as PageId };
}

export function Sidebar() {
  const location = useApp((s) => s.location);
  const goTo = useApp((s) => s.goTo);
  const collapsed = useApp((s) => s.sidebarCollapsed);
  const toggle = useApp((s) => s.toggleSidebar);
  const project = useApp((s) => s.project);
  const runs = useExperiments((s) => s.runs.length);

  const sections: SidebarNavSection<NavId>[] = [
    {
      items: [
        { id: "overview", label: "Overview", icon: "home" },
        { id: "projects", label: "Projects", icon: "folder-open" },
      ],
    },
    {
      title: "Research",
      items: [
        { id: "ws:studio", label: "Studio", icon: "grid" },
        { id: "ws:data", label: "Datasets", icon: "database", meta: project?.datasets.length || undefined },
        { id: "ws:experiments", label: "Experiments", icon: "chart-line", meta: runs || undefined },
        { id: "ws:models", label: "Models", icon: "brain" },
        { id: "ws:config", label: "Configs", icon: "settings", meta: project?.trainConfigs.length || undefined },
        { id: "pipelines", label: "Pipelines", icon: "nodes" },
      ],
    },
    { items: [{ id: "settings", label: "Settings", icon: "wrench" }] },
  ];

  return (
    <SidebarNav<NavId>
      label="Epoch"
      sections={sections}
      value={toId(location)}
      onChange={(id) => goTo(toLocation(id))}
      collapsed={collapsed}
      className="border-r border-border/60"
      footer={
        <div className={cn("flex p-2", collapsed ? "justify-center" : "justify-end")}>
          <Button variant="ghost" size="icon-xs" aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"} onClick={toggle}>
            <Icon name={collapsed ? "sidebar-right" : "sidebar-left"} />
          </Button>
        </div>
      }
    />
  );
}
