import type * as React from "react";
import { Button, cn, Icon, type IconName, Tooltip, TooltipContent, TooltipTrigger } from "@nexis/design";

/** The strip at the top of a panel: what it shows, then its actions. */
export function PanelToolbar({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <div
      role="toolbar"
      className={cn("flex h-9 shrink-0 items-center gap-1.5 border-b border-border/60 px-2 text-xs", className)}
    >
      {children}
    </div>
  );
}

export function ToolbarTitle({ icon, children, meta }: { icon?: IconName; children: React.ReactNode; meta?: React.ReactNode }) {
  return (
    <div className="flex min-w-0 items-center gap-1.5 pl-1">
      {icon && <Icon name={icon} size="xs" className="shrink-0 text-muted-foreground" />}
      <span className="truncate font-medium text-foreground">{children}</span>
      {meta !== undefined && <span className="shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums">{meta}</span>}
    </div>
  );
}

export function ToolbarButton({
  icon,
  label,
  onClick,
  disabled,
  active,
}: {
  icon: IconName;
  label: string;
  onClick(): void;
  disabled?: boolean;
  active?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label={label}
          aria-pressed={active}
          disabled={disabled}
          onClick={onClick}
          className={cn(active && "bg-muted text-foreground")}
        >
          <Icon name={icon} active={active} />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
