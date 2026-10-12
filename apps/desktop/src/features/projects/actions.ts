/**
 * Opening and closing projects. Opening reads the folder (detection, the
 * experiment index); it never runs anything in it.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "@nexis/design";
import { useApp } from "@/app/store";
import { useExperiments } from "@/features/experiments/store";
import { api } from "@/lib/api";
import { BACKEND_AVAILABLE, errorMessage } from "@/lib/ipc";

export async function refreshRecent(): Promise<void> {
  try {
    useApp.setState({ recent: await api.projectRecent() });
  } catch {
    // The list is a convenience; the shell works without it.
  }
}

export async function openProject(path: string): Promise<boolean> {
  useApp.setState({ projectBusy: true });
  try {
    const { summary, index } = await api.projectOpen(path);
    useApp.setState({ project: summary });
    useExperiments.getState().reset();
    void useExperiments.getState().reload();
    void refreshRecent();
    const facts = [
      summary.datasets.length && `${summary.datasets.length} dataset${summary.datasets.length === 1 ? "" : "s"}`,
      summary.trainConfigs.length && `${summary.trainConfigs.length} train.toml`,
      index && `${index.runs} run${index.runs === 1 ? "" : "s"} indexed`,
    ].filter(Boolean);
    toast.success(`Opened ${summary.name}`, { description: facts.join(" · ") || "No datasets or runs found yet." });
    return true;
  } catch (e) {
    toast.error("Couldn't open the project", { description: errorMessage(e) });
    return false;
  } finally {
    useApp.setState({ projectBusy: false });
  }
}

export async function chooseAndOpenProject(): Promise<void> {
  if (!BACKEND_AVAILABLE) {
    toast("Open a project in the desktop app", {
      description: "This browser preview has no access to your files. Run `pnpm tauri:dev`.",
    });
    return;
  }
  const dir = await open({ directory: true, multiple: false, title: "Open a project folder" });
  if (typeof dir === "string") await openProject(dir);
}

export async function closeProject(): Promise<void> {
  const dirty = Object.keys(useApp.getState().dirty).length;
  if (dirty > 0) {
    toast.warning("Save or discard your changes first", {
      description: `${dirty} panel${dirty === 1 ? " has" : "s have"} unsaved edits.`,
    });
    return;
  }
  await api.projectClose().catch(() => {});
  useApp.setState({ project: null });
  useExperiments.getState().reset();
}

export async function rescanProject(): Promise<void> {
  try {
    const summary = await api.projectRescan();
    useApp.setState({ project: summary });
    const msg = await useExperiments.getState().reindex();
    toast.success("Project rescanned", { description: msg });
  } catch (e) {
    toast.error("Rescan failed", { description: errorMessage(e) });
  }
}

/** At startup: settings, recent projects, and a project the backend still
 * has open (after a webview reload). */
export async function bootstrap(): Promise<void> {
  if (!BACKEND_AVAILABLE) return;
  try {
    const loaded = await api.settingsGet();
    useApp.setState({ settings: loaded.settings, settingsWarnings: loaded.warnings });
    for (const w of loaded.warnings) toast.warning("Settings", { description: w });
  } catch (e) {
    toast.error("Couldn't read settings", { description: errorMessage(e) });
  }
  void refreshRecent();
  try {
    const current = await api.projectCurrent();
    if (current) {
      useApp.setState({ project: current });
      void useExperiments.getState().reload();
    }
  } catch {
    // no project
  }
}
