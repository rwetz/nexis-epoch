/**
 * One typed function per Tauri command. Panels import from here, never
 * `invoke` directly, so the command surface is greppable in one file.
 */
import type {
  AppInfo,
  AppSettings,
  DatasetInfo,
  DirListing,
  FileIndex,
  IndexedRun,
  IndexStats,
  LoadedSettings,
  Opened,
  Page,
  PageRequest,
  ParseReport,
  PathSeg,
  ProjectSummary,
  QueryResult,
  RecentProject,
  RunDetail,
  ScalarValue,
  SystemStatus,
  TextFile,
} from "@/types/backend";
import { call } from "./ipc";

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  settingsGet: () => call<LoadedSettings>("settings_get"),
  settingsSave: (settings: AppSettings) => call<AppSettings>("settings_save", { settings }),

  layoutLoad: (workspace: string) => call<string | null>("layout_load", { workspace }),
  layoutSave: (workspace: string, json: string) => call<void>("layout_save", { workspace, json }),
  layoutDelete: (workspace: string) => call<void>("layout_delete", { workspace }),

  projectOpen: (path: string) => call<Opened>("project_open", { path }),
  projectClose: () => call<void>("project_close"),
  projectCurrent: () => call<ProjectSummary | null>("project_current"),
  projectRescan: () => call<ProjectSummary>("project_rescan"),
  projectRecent: () => call<RecentProject[]>("project_recent"),
  projectForget: (path: string) => call<void>("project_forget", { path }),
  projectFiles: () => call<FileIndex>("project_files"),

  listDir: (path: string) => call<DirListing>("fs_list_dir", { path }),
  readText: (path: string) => call<TextFile>("fs_read_text", { path }),
  writeText: (path: string, content: string, expectedHash: string | null) =>
    call<TextFile>("fs_write_text", { path, content, expectedHash }),

  tomlParse: (text: string) => call<ParseReport>("toml_parse", { text }),
  tomlSetValue: (text: string, path: PathSeg[], value: ScalarValue) =>
    call<string>("toml_set_value", { text, path, value }),

  datasetDescribe: (path: string, queryId?: string) => call<DatasetInfo>("dataset_describe", { path, queryId }),
  datasetPage: (path: string, request: PageRequest, queryId?: string) =>
    call<Page>("dataset_page", { path, request, queryId }),
  datasetStats: (path: string, queryId?: string) => call<QueryResult>("dataset_stats", { path, queryId }),
  sqlRun: (sql: string, queryId?: string) => call<QueryResult>("sql_run", { sql, queryId }),
  sqlRegisterView: (path: string, name: string) => call<string>("sql_register_view", { path, name }),
  queryCancel: (queryId: string) => call<void>("query_cancel", { queryId }),

  experimentsIndex: () => call<IndexStats>("experiments_index"),
  experimentsList: () => call<IndexedRun[]>("experiments_list"),
  experimentDetail: (runDir: string) => call<RunDetail>("experiment_detail", { runDir }),
  experimentArtifact: (runDir: string, path: string) =>
    call<unknown>("experiment_artifact", { runDir, path }),

  systemStatus: () => call<SystemStatus>("system_status"),
};
