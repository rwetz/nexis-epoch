import * as React from "react";
import {
  Button,
  Field,
  Icon,
  Input,
  listCommunityThemes,
  listNexisThemes,
  NumberInput,
  PropertyList,
  Segmented,
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
  Switch,
  toast,
  useTheme,
} from "@nexis/design";
import { confirmAction } from "@/components/confirm";
import { EmptyState } from "@/components/states";
import { WORKSPACES } from "@/features/workspace/presets";
import { deleteLayout } from "@/features/workspace/persistence";
import { api } from "@/lib/api";
import { BACKEND_AVAILABLE, errorMessage } from "@/lib/ipc";
import type { AppInfo, AppSettings } from "@/types/backend";
import { useApp } from "../store";

function Section({ title, description, children }: { title: string; description?: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-4 rounded-xl border border-border/60 bg-card p-5">
      <header>
        <h2 className="font-heading text-sm font-semibold">{title}</h2>
        {description && <p className="text-xs text-muted-foreground">{description}</p>}
      </header>
      {children}
    </section>
  );
}

function Appearance() {
  const theme = useTheme();
  return (
    <Section title="Appearance" description="Stored on this device.">
      <Field label="Theme" inline>
        <Select value={theme.themeId} onValueChange={theme.setThemeId}>
          <SelectTrigger className="w-64">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              <SelectLabel>Nexis</SelectLabel>
              {listNexisThemes().map((t) => (
                <SelectItem key={t.id} value={t.id}>
                  {t.name}
                </SelectItem>
              ))}
            </SelectGroup>
            <SelectGroup>
              <SelectLabel>Community</SelectLabel>
              {listCommunityThemes().map((t) => (
                <SelectItem key={t.id} value={t.id}>
                  {t.name}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      </Field>
      <Field label="Mode" inline>
        <Segmented
          label="Mode"
          value={theme.mode}
          onChange={theme.setMode}
          segments={[
            { id: "system", label: "System" },
            { id: "light", label: "Light", icon: "theme-light" },
            { id: "dark", label: "Dark", icon: "theme-dark" },
          ]}
        />
      </Field>
      <Field label="High contrast" inline hint="Stronger borders and text on top of any theme.">
        <Switch checked={theme.contrast === "high"} onCheckedChange={(on) => theme.setContrast(on ? "high" : "standard")} />
      </Field>
    </Section>
  );
}

function WorkspaceSection() {
  return (
    <Section title="Workspaces" description="Layouts are saved per workspace in the app data folder, as versioned JSON.">
      <Field label="Saved layouts" inline hint="Takes effect the next time Epoch starts. Your panels' files are not affected.">
        <Button
          variant="outline"
          size="sm"
          className="w-fit"
          onClick={async () => {
            const ok = await confirmAction({
              title: "Forget every saved layout?",
              description: "All five workspaces will start from their default layout next time.",
              confirmLabel: "Forget layouts",
              destructive: true,
            });
            if (!ok) return;
            await Promise.all(WORKSPACES.map((w) => deleteLayout(w.id).catch(() => {})));
            toast.success("Saved layouts cleared", { description: "Restart Epoch to see the defaults." });
          }}
        >
          <Icon name="revert" /> Forget saved layouts
        </Button>
      </Field>
    </Section>
  );
}

function BackendSettings() {
  const saved = useApp((s) => s.settings);
  const [draft, setDraft] = React.useState<AppSettings | null>(saved);
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => setDraft(saved), [saved]);
  if (!draft || !saved) {
    return (
      <Section title="Data, Python and experiments">
        <EmptyState icon="computer" title="Available in the desktop app" description="These settings live in settings.toml, written by the Rust backend." className="min-h-28" />
      </Section>
    );
  }
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const set = <K extends keyof AppSettings>(section: K, patch: Partial<AppSettings[K]>) =>
    setDraft((d) => (d ? { ...d, [section]: { ...d[section], ...patch } } : d));
  const save = async () => {
    setSaving(true);
    try {
      const settings = await api.settingsSave(draft);
      useApp.setState({ settings, settingsWarnings: [] });
      toast.success("Settings saved");
    } catch (e) {
      toast.error("Couldn't save settings", { description: errorMessage(e) });
    } finally {
      setSaving(false);
    }
  };
  return (
    <Section title="Data, Python and experiments" description="Saved to settings.toml; comments you add to that file are kept.">
      <Field label="Rows per page" inline hint="Fetched at a time as you scroll a dataset.">
        <NumberInput value={draft.data.pageSize} onChange={(v) => set("data", { pageSize: v })} min={25} max={2000} step={25} />
      </Field>
      <Field label="Query row limit" inline hint="Most rows an SQL query returns.">
        <NumberInput value={draft.data.queryRowLimit} onChange={(v) => set("data", { queryRowLimit: v })} min={100} max={200000} step={1000} />
      </Field>
      <Field label="Query timeout" inline>
        <NumberInput value={draft.data.queryTimeoutSecs} onChange={(v) => set("data", { queryTimeoutSecs: v })} min={1} max={3600} suffix="s" />
      </Field>
      <Field label="DuckDB memory limit" inline>
        <NumberInput value={draft.data.memoryLimitMb} onChange={(v) => set("data", { memoryLimitMb: v })} min={256} max={262144} step={256} suffix="MB" />
      </Field>
      <Field label="DuckDB threads" inline hint="0 lets DuckDB choose.">
        <NumberInput value={draft.data.threads} onChange={(v) => set("data", { threads: v })} min={0} max={256} />
      </Field>
      <Field label="Python interpreter" inline hint="Used for future script runs. Empty: use the project's detected environment.">
        <Input
          value={draft.python.interpreter}
          onChange={(e) => set("python", { interpreter: e.target.value })}
          placeholder="e.g. C:\\Python312\\python.exe"
          className="font-mono text-xs"
        />
      </Field>
      <Field label="Show hidden files" inline>
        <Switch checked={draft.explorer.showHidden} onCheckedChange={(v) => set("explorer", { showHidden: v })} />
      </Field>
      <Field label="Index runs on open" inline hint="Scan Nexis ML run stores when a project opens.">
        <Switch checked={draft.experiments.indexOnOpen} onCheckedChange={(v) => set("experiments", { indexOnOpen: v })} />
      </Field>
      <div className="flex justify-end gap-2 border-t border-border/60 pt-4">
        <Button variant="ghost" size="sm" disabled={!dirty || saving} onClick={() => setDraft(saved)}>
          Discard
        </Button>
        <Button variant="brand" size="sm" disabled={!dirty || saving} onClick={() => void save()}>
          <Icon name="save" /> {saving ? "Saving…" : "Save"}
        </Button>
      </div>
    </Section>
  );
}

function About() {
  const [info, setInfo] = React.useState<AppInfo | null>(null);
  React.useEffect(() => {
    if (BACKEND_AVAILABLE) api.appInfo().then(setInfo, () => {});
  }, []);
  return (
    <Section title="About">
      <PropertyList
        items={[
          { label: "Version", value: info?.version ?? "browser preview", mono: true },
          { label: "Settings file", value: info?.settingsFile ?? "—", mono: true },
          { label: "Data folder", value: info?.dataDir ?? "—", mono: true },
          { label: "Workspace layout", value: "dockview (MIT)" },
        ]}
      />
    </Section>
  );
}

export function SettingsPage() {
  return (
    <div className="nexis-scrollbar h-full overflow-y-auto">
      <div className="mx-auto flex w-full max-w-3xl flex-col gap-4 px-6 py-6">
        <h1 className="font-heading text-xl font-semibold tracking-tight">Settings</h1>
        <BackendSettings />
        <Appearance />
        <WorkspaceSection />
        <About />
      </div>
    </div>
  );
}
