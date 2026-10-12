/**
 * CodeMirror 6 for TOML and SQL. Colours are CSS variables, so the editor
 * follows every theme without being rebuilt.
 */
import * as React from "react";
import { autocompletion, closeBrackets } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { sql, StandardSQL } from "@codemirror/lang-sql";
import {
  bracketMatching,
  foldGutter,
  HighlightStyle,
  indentOnInput,
  StreamLanguage,
  syntaxHighlighting,
} from "@codemirror/language";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { type Diagnostic as LintDiagnostic, linter, lintGutter, setDiagnostics } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import { cn } from "@nexis/design";

export type EditorLanguage = "toml" | "sql";

export type EditorDiagnostic = {
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  message: string;
  severity?: "error" | "warning";
};

export type CodeEditorHandle = {
  goToLine(line: number): void;
  focus(): void;
};

const theme = EditorView.theme({
  "&": { backgroundColor: "var(--card)", color: "var(--foreground)" },
  ".cm-content": { caretColor: "var(--brand)", padding: "6px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--brand)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: "color-mix(in oklab, var(--brand) 26%, transparent) !important",
  },
  ".cm-gutters": {
    backgroundColor: "var(--card)",
    color: "color-mix(in oklab, var(--muted-foreground) 70%, transparent)",
    border: "none",
  },
  ".cm-activeLine": { backgroundColor: "color-mix(in oklab, var(--foreground) 4%, transparent)" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--foreground)" },
  ".cm-tooltip": {
    backgroundColor: "var(--popover)",
    color: "var(--popover-foreground)",
    border: "1px solid var(--border)",
    borderRadius: "8px",
  },
  ".cm-tooltip-autocomplete ul li[aria-selected]": { backgroundColor: "var(--accent)", color: "var(--accent-foreground)" },
  ".cm-diagnostic-error": { borderLeftColor: "var(--destructive)" },
  ".cm-lintRange-error": {
    backgroundImage: "none",
    textDecoration: "underline wavy var(--destructive)",
    textUnderlineOffset: "3px",
  },
  ".cm-panels": { backgroundColor: "var(--popover)", color: "var(--popover-foreground)" },
  ".cm-searchMatch": { backgroundColor: "color-mix(in oklab, var(--warning) 30%, transparent)" },
  ".cm-foldGutter span": { color: "var(--muted-foreground)" },
});

const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.operatorKeyword, t.modifier], color: "var(--terminal-ansi-magenta)" },
  { tag: [t.string, t.special(t.string)], color: "var(--terminal-ansi-green)" },
  { tag: [t.number, t.bool, t.null, t.atom], color: "var(--terminal-ansi-yellow)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--muted-foreground)", fontStyle: "italic" },
  { tag: [t.propertyName, t.definition(t.propertyName), t.variableName], color: "var(--terminal-ansi-blue)" },
  { tag: [t.heading, t.typeName, t.className, t.meta], color: "var(--brand)", fontWeight: "600" },
  { tag: [t.function(t.variableName), t.standard(t.name)], color: "var(--terminal-ansi-cyan)" },
  { tag: t.invalid, color: "var(--destructive)" },
]);

function languageExtension(lang: EditorLanguage, schema?: Record<string, string[]>): Extension {
  if (lang === "toml") return StreamLanguage.define(toml);
  // CodeMirror has no DuckDB dialect; standard SQL is the closest.
  return sql({ dialect: StandardSQL, schema, upperCaseKeywords: true });
}

function toLint(doc: EditorState["doc"], d: EditorDiagnostic): LintDiagnostic {
  const clampLine = (n: number) => doc.line(Math.min(Math.max(1, n), doc.lines));
  const start = clampLine(d.line);
  const end = clampLine(d.endLine);
  const from = Math.min(start.to, start.from + Math.max(0, d.column - 1));
  const to = Math.max(from + 1, Math.min(end.to, end.from + Math.max(0, d.endColumn - 1)));
  return { from, to: Math.min(to, doc.length), severity: d.severity ?? "error", message: d.message };
}

export const CodeEditor = React.forwardRef<
  CodeEditorHandle,
  {
    value: string;
    onChange(value: string): void;
    language: EditorLanguage;
    diagnostics?: readonly EditorDiagnostic[];
    /** Mod+Enter. */
    onRun?(): void;
    /** Mod+S. */
    onSave?(): void;
    /** SQL completion: table name → columns. */
    schema?: Record<string, string[]>;
    readOnly?: boolean;
    className?: string;
    label: string;
  }
>(function CodeEditor({ value, onChange, language, diagnostics, onRun, onSave, schema, readOnly, className, label }, ref) {
  const host = React.useRef<HTMLDivElement>(null);
  const view = React.useRef<EditorView | null>(null);
  const callbacks = React.useRef({ onChange, onRun, onSave });
  callbacks.current = { onChange, onRun, onSave };
  const langSlot = React.useRef(new Compartment());
  const readOnlySlot = React.useRef(new Compartment());

  React.useImperativeHandle(ref, () => ({
    goToLine(line: number) {
      const v = view.current;
      if (!v) return;
      const l = v.state.doc.line(Math.min(Math.max(1, line), v.state.doc.lines));
      v.dispatch({ selection: { anchor: l.from }, effects: EditorView.scrollIntoView(l.from, { y: "center" }) });
      v.focus();
    },
    focus() {
      view.current?.focus();
    },
  }));

  React.useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          foldGutter(),
          lintGutter(),
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          autocompletion(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          highlightSelectionMatches(),
          syntaxHighlighting(highlight),
          theme,
          linter(null),
          EditorView.contentAttributes.of({ "aria-label": label }),
          langSlot.current.of(languageExtension(language, schema)),
          readOnlySlot.current.of(EditorState.readOnly.of(!!readOnly)),
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                callbacks.current.onRun?.();
                return !!callbacks.current.onRun;
              },
            },
            {
              key: "Mod-s",
              preventDefault: true,
              run: () => {
                callbacks.current.onSave?.();
                return true;
              },
            },
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) callbacks.current.onChange(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // Created once; later prop changes are dispatched below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // External edits (a structured edit, a reload) replace the document.
  React.useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== value) {
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: value } });
    }
  }, [value]);

  React.useEffect(() => {
    view.current?.dispatch({ effects: langSlot.current.reconfigure(languageExtension(language, schema)) });
  }, [language, schema]);

  React.useEffect(() => {
    view.current?.dispatch({ effects: readOnlySlot.current.reconfigure(EditorState.readOnly.of(!!readOnly)) });
  }, [readOnly]);

  React.useEffect(() => {
    const v = view.current;
    if (!v) return;
    v.dispatch(setDiagnostics(v.state, (diagnostics ?? []).map((d) => toLint(v.state.doc, d))));
  }, [diagnostics, value]);

  return <div ref={host} className={cn("epoch-editor min-h-0 flex-1 overflow-hidden", className)} />;
});
