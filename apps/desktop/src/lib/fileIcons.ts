import type { IconName } from "@nexis/design";
import type { FileKind } from "@/types/backend";

const ICONS: Record<FileKind, IconName> = {
  directory: "folder",
  toml: "settings",
  python: "brand-python",
  notebook: "note",
  csv: "table",
  parquet: "database",
  json: "code-box",
  jsonl: "code-box",
  sqlite: "database",
  checkpoint: "brain",
  image: "image",
  markdown: "document",
  text: "file",
  other: "file",
};

export function fileIcon(kind: FileKind, open = false): IconName {
  if (kind === "directory" && open) return "folder-open";
  return ICONS[kind];
}

export function isTabular(kind: FileKind): boolean {
  return kind === "csv" || kind === "parquet" || kind === "json" || kind === "jsonl";
}

export function basename(path: string): string {
  const parts = path.split(/[\/]/);
  return parts[parts.length - 1] || path;
}
