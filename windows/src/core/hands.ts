// Mochi's hands, as the island shows them (src-tauri/src/agent/): the steps of a turn in the chat, and the card of
// an action waiting for a click. Pure data and helpers, so they can be tested without a webview; the chat view
// draws them (views/chat.ts) and island/hands.ts listens to Rust.

import { fromEdit, fromNew, type DiffKind } from "./diff";
import { N_, t, tn } from "../i18n/i18n";

export type HandsStatus = "running" | "waiting" | "ok" | "error" | "denied";

/** One thing Mochi does, as Rust reports it (agent/run.rs `Step`). */
export interface HandsStep {
  index: number;
  tool: string;
  /** The command, the path or the address, on one line. */
  summary: string;
  status: HandsStatus;
  /** "exit 0", "120" (lines), "+3 −1"… */
  detail?: string;
  /** A command's last lines, or why it failed. */
  output?: string;
}

/** An action waiting for a click (agent/tools.rs `Request`, plus where it came from). */
export interface HandsRequest {
  id: string;
  run: number;
  origin: "island" | "phone";
  tool: string;
  summary: string;
  cwd?: string;
  /** For a file change: the text before (absent for a new file) and after, when small enough to show. */
  before?: string;
  after?: string;
  /** A write that creates the file, shown or not. */
  newFile: boolean;
  outside: boolean;
  secret: boolean;
}

/** What Settings → Mochi's hands and the island know (agent::AgentStatus). */
export interface HandsStatusReport {
  enabled: boolean;
  folder: string;
  level: "read" | "ask" | "edits";
  shell: string;
  running: "island" | "phone" | null;
  waiting: HandsRequest | null;
  suggested: string;
}

/** The verb of each tool in a step row; the same words as the agents' steps on the island. */
const VERBS: Record<string, string> = {
  run_command: N_("Runs"),
  read_file: N_("Reads"),
  write_file: N_("Writes"),
  edit_file: N_("Edits"),
  list_directory: N_("Lists"),
  find_files: N_("Searches"),
  search_text: N_("Searches"),
  fetch_url: N_("Fetches"),
  open_url: N_("Opens"),
  system_info: N_("Looks at this computer"),
  home_devices: N_("Looks at the house"),
  home_control: N_("Controls"),
  home_routine: N_("Runs the routine"),
};

export function stepVerb(tool: string): string {
  const verb = VERBS[tool];
  return verb ? t(verb) : tool;
}

/** The detail shown at the end of a step row: a command's exit code, a file's line count, an edit's counts. */
export function stepDetail(step: HandsStep): string {
  const d = step.detail ?? "";
  if (!d) return "";
  const n = Number(d);
  if (Number.isInteger(n)) {
    switch (step.tool) {
      case "read_file":
      case "write_file":
        return tn("{count} line", "{count} lines", n);
      case "list_directory":
        return tn("{count} item", "{count} items", n);
      case "find_files":
        return tn("{count} file", "{count} files", n);
      case "search_text":
        return tn("{count} match", "{count} matches", n);
      case "home_devices":
        return tn("{count} device", "{count} devices", n);
    }
  }
  if (d === "timeout") return t("time limit");
  if (d === "stopped") return t("stopped");
  return d;
}

/** The title of an action's card: what Mochi wants to do, in words. */
export function requestTitle(r: HandsRequest): string {
  switch (r.tool) {
    case "run_command":
      return t("Mochi wants to run a command");
    case "write_file":
      return r.newFile ? t("Mochi wants to create a file") : t("Mochi wants to replace a file");
    case "edit_file":
      return t("Mochi wants to edit a file");
    case "fetch_url":
      return t("Mochi wants to fetch a web page");
    case "open_url":
      return t("Mochi wants to open a web page");
    case "home_control":
      return t("Mochi wants to control a device in your home");
    case "home_routine":
      return t("Mochi wants to run a routine");
    default:
      return r.secret ? t("Mochi wants to read a file that may hold secrets") : t("Mochi wants to look outside his folder");
  }
}

/** Why it asks, when that is not the action itself: a path outside the working folder or a secret-looking file. */
export function requestWarning(r: HandsRequest): string | null {
  if (r.secret) return t("This file looks like it holds keys or passwords.");
  if (r.outside) return t("Outside the folder Mochi works in.");
  return null;
}

export interface PreviewLine {
  kind: DiffKind | "gap";
  text: string;
}

/** A file change as a few lines of diff for the card: the changed lines with a line of context, at most `max`. */
export function changePreview(r: HandsRequest, max = 14): { added: number; removed: number; lines: PreviewLine[]; more: number } | null {
  if (r.after == null) return null;
  const diff = r.before == null ? fromNew(r.after, r.summary) : fromEdit(r.before, r.after, r.summary);
  const lines: PreviewLine[] = [];
  let total = 0;
  diff.hunks.forEach((hunk, i) => {
    if (i > 0) {
      total++;
      if (lines.length < max) lines.push({ kind: "gap", text: "" });
    }
    for (const line of hunk.lines) {
      total++;
      if (lines.length < max) lines.push({ kind: line.kind, text: line.text });
    }
  });
  return { added: diff.added, removed: diff.removed, lines, more: Math.max(0, total - lines.length) };
}

/** Upserts a step into a list kept in step order (a step's later states replace its earlier ones). */
export function upsertStep(steps: HandsStep[], step: HandsStep): HandsStep[] {
  const next = steps.filter((s) => s.index !== step.index);
  next.push(step);
  next.sort((a, b) => a.index - b.index);
  return next;
}
