import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  fmtShortcut,
  Icon,
  Kbd,
  MOD_KEY,
  TitleBar,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@nexis/design";
import { AppLogo } from "@epoch/AppLogo";
import { WORKSPACES } from "@epoch/features/workspace/presets";
import { useApp, workspaceApi } from "./store";

function WorkspaceSelector() {
  const location = useApp((s) => s.location);
  const last = useApp((s) => s.lastWorkspace);
  const goTo = useApp((s) => s.goTo);
  const current = WORKSPACES.find((w) => w.id === (location.kind === "workspace" ? location.workspace : last))!;
  const inWorkspace = location.kind === "workspace";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="xs" className="gap-1.5 text-[12.5px]" aria-label="Choose workspace">
          <Icon name={current.icon} size="xs" className={inWorkspace ? "text-brand" : undefined} />
          {current.name}
          <Icon name="chevron-down" size={10} className="text-muted-foreground" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72">
        <DropdownMenuLabel className="text-[11px]">Workspaces</DropdownMenuLabel>
        {WORKSPACES.map((w) => (
          <DropdownMenuItem key={w.id} onSelect={() => goTo({ kind: "workspace", workspace: w.id })} className="items-start">
            <Icon name={w.icon} className="mt-0.5" />
            <span className="flex flex-col">
              <span className="font-medium">{w.name}</span>
              <span className="text-[11px] text-muted-foreground">{w.description}</span>
            </span>
          </DropdownMenuItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={!inWorkspace} onSelect={() => void workspaceApi()?.resetLayout()}>
          <Icon name="revert" /> Reset this layout
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function TopBar({ onSearch }: { onSearch(): void }) {
  const project = useApp((s) => s.project);
  const goTo = useApp((s) => s.goTo);
  return (
    <TitleBar
      brand={<AppLogo className="size-[18px] text-brand" />}
      title={
        <span className="flex items-center gap-1.5">
          <span className="font-heading font-semibold tracking-tight">Epoch</span>
          <span className="text-muted-foreground/60">/</span>
          <span className={project ? "text-foreground" : "text-muted-foreground"}>{project?.name ?? "No project"}</span>
        </span>
      }
      center={
        <div className="flex w-full max-w-xl items-center gap-2">
          <WorkspaceSelector />
          <button
            type="button"
            onClick={onSearch}
            className="flex h-7 min-w-0 flex-1 items-center gap-2 rounded-full bg-muted/70 px-3 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
          >
            <Icon name="search" size="xs" />
            <span className="flex-1 truncate text-left">{project ? `Search ${project.name} and commands` : "Search commands"}</span>
            <Kbd>{fmtShortcut(MOD_KEY, "P")}</Kbd>
          </button>
        </div>
      }
      actions={
        <>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon-xs" aria-label="Settings" onClick={() => goTo({ kind: "page", page: "settings" })}>
                <Icon name="settings" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Settings {fmtShortcut(MOD_KEY, ",")}</TooltipContent>
          </Tooltip>
        </>
      }
    />
  );
}
