import type { StartupView } from "../types";

type RestorableListView = StartupView | "trash";

export type StartupTarget =
  | { mode: RestorableListView }
  | { mode: "notebook"; notebookPath: string }
  | { mode: "tag"; tag: string };

export interface StartupState {
  startupView: unknown;
  restoreLastSession: boolean;
  lastViewMode: unknown;
  lastNotebook: string | null;
  lastTag: string | null;
}

export function normalizeStartupView(value: unknown): StartupView {
  switch (value) {
    case "daily":
    case "quickaccess":
    case "tasks":
      return value;
    default:
      return "all";
  }
}

function restorableListView(value: unknown): RestorableListView | null {
  switch (value) {
    case "all":
    case "daily":
    case "quickaccess":
    case "tasks":
    case "trash":
      return value;
    default:
      return null;
  }
}

export function resolveStartupTarget(state: StartupState): StartupTarget {
  const fallback: StartupTarget = {
    mode: normalizeStartupView(state.startupView),
  };
  if (!state.restoreLastSession) return fallback;

  if (state.lastViewMode === "notebook" && typeof state.lastNotebook === "string") {
    return { mode: "notebook", notebookPath: state.lastNotebook };
  }
  if (state.lastViewMode === "tag" && state.lastTag) {
    return { mode: "tag", tag: state.lastTag };
  }

  const listView = restorableListView(state.lastViewMode);
  return listView ? { mode: listView } : fallback;
}
