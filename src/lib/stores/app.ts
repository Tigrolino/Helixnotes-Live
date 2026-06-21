import { writable, derived, get } from "svelte/store";
import type {
  AppConfig,
  NoteEntry,
  NoteContent,
  NotebookEntry,
  VaultState,
  ViewMode,
  SortMode,
} from "$lib/types";

// App state
export const appConfig = writable<AppConfig | null>(null);
export const vaultReady = writable(false);

// UI state
export const viewMode = writable<ViewMode>("all");
export const sortMode = writable<SortMode>("modified");
export const tasksLayout = writable<"list" | "calendar">("list");
export const tasksHideCompleted = writable<boolean>(true);
export const tasksOnlyFlagged = writable<boolean>(false);
export const tasksSort = writable<"due" | "priority" | "note">("due");
export const sidebarCollapsed = writable(false);
export const sidebarWidth = writable(220);
export const notelistWidth = writable(280);
export const searchQuery = writable("");
export const showCommandPalette = writable(false);
export const showSearch = writable(false);
export const showSettings = writable(false);
export const settingsTab = writable<string | null>(null);
export const showInfo = writable(false);
export const notebookIcons = writable<Record<string, string>>({});
export const quickAccessPaths = writable<string[]>([]);
export const collapsedNotebooks = writable<string[]>([]);

// Notebook sort: 'alphabetical' (default) or 'manual' (drag-to-reorder, persisted in notebookOrder).
export const notebookSortMode = writable<"alphabetical" | "manual">(
  "alphabetical",
);
// Map from notebook absolute path → ordinal position (lower = earlier). Only consulted when notebookSortMode === 'manual'.
export const notebookOrder = writable<Record<string, number>>({});
export const noteOrder = writable<Record<string, number>>({});

// Data
export const notebooks = writable<NotebookEntry[]>([]);
export const rootNoteCount = writable<number>(0);
export const notes = writable<NoteEntry[]>([]);
export const tags = writable<[string, number][]>([]);
export const activeNote = writable<NoteContent | null>(null);
export const activeNotePath = writable<string | null>(null);
export const activeNotebook = writable<NotebookEntry | null>(null);
export const activeTag = writable<string | null>(null);

// External-file viewer mode: set when an .md file outside the active vault is opened.
// While set, the editor is forced read-only, autosave is suppressed, and a banner
// offering "Import to vault" / "Close" is shown.
export const viewerNote = writable<{ path: string; content: string } | null>(
  null,
);

// Mobile state
export const mobileView = writable<"sidebar" | "notelist" | "editor">(
  "sidebar",
);

// Editor state
export const editorDirty = writable(false);
export const sourceMode = writable(false);
export const focusMode = writable(false);
export const readOnly = writable(false);

// Theme
export const theme = writable<string>("system");

// Sync (WebDAV) - global status so the top-bar button reflects any sync,
// whoever triggered it (manual button, settings, interval, on-change).
export const syncState = writable<{ running: boolean; error: string | null }>({
  running: false,
  error: null,
});

// Update state
export const updateAvailable = writable<{
  version: string;
  body?: string;
} | null>(null);
export const updateObj = writable<any>(null);
export const installType = writable<string>("native");
// True only on the Android/iOS build. Defaults to a user-agent guess for the first paint, then
// gets the authoritative compile-time value from the backend at startup (see +layout). (#63)
export const platformIsMobile = writable<boolean>(
  typeof navigator !== "undefined" && /android|iphone|ipad|ipod/i.test(navigator.userAgent),
);
export const androidApkUrl = writable<string | null>(null);

// Install types that handle their own updates (in-app auto-updater, or a
// package-manager notice for deb/aur). Anything else - e.g. a distro repo build
// that sets the HELIXNOTES_INSTALL_TYPE build flag (Solus, etc.) - is "managed":
// the app does no update check and shows no update UI at all.
const SELF_UPDATING_INSTALL_TYPES = ["macos", "windows", "deb", "aur", "appimage", "native", "android"];
export function isManagedInstall(type: string): boolean {
  return !SELF_UPDATING_INSTALL_TYPES.includes(type);
}

export async function checkForUpdate() {
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (update) {
      updateAvailable.set({ version: update.version, body: update.body });
      updateObj.set(update);
    }
  } catch {
    // Silent fail - don't disrupt app startup
  }
}

function isNewerVersion(remote: string, local: string): boolean {
  const r = remote.split(".").map(Number);
  const l = local.split(".").map(Number);
  for (let i = 0; i < Math.max(r.length, l.length); i++) {
    const rv = r[i] || 0;
    const lv = l[i] || 0;
    if (rv > lv) return true;
    if (rv < lv) return false;
  }
  return false;
}

export async function checkForUpdateMobile() {
  try {
    const { getVersion } = await import("@tauri-apps/api/app");
    const currentVersion = await getVersion();
    const res = await fetch("https://helixnotes.com/latest.json");
    if (!res.ok) return;
    const data = await res.json();
    if (data.version && isNewerVersion(data.version, currentVersion)) {
      updateAvailable.set({ version: data.version, body: data.notes });
      const android = data.platforms?.["android-universal"];
      if (android?.url) {
        androidApkUrl.set(android.url);
      }
    }
  } catch {
    // Silent fail
  }
}

// Note navigation history
interface NavHistoryState { stack: string[]; index: number; skipping: boolean; }
function createNavHistory() {
  const store = writable<NavHistoryState>({ stack: [], index: -1, skipping: false });
  return {
    subscribe: store.subscribe,
    push(path: string) {
      store.update(s => {
        if (s.skipping) return { ...s, skipping: false };
        const trimmed = s.stack.slice(0, s.index + 1);
        return { stack: [...trimmed, path], index: trimmed.length, skipping: false };
      });
    },
    go(direction: -1 | 1): string | null {
      let target: string | null = null;
      store.update(s => {
        const newIdx = s.index + direction;
        if (newIdx < 0 || newIdx >= s.stack.length) return s;
        target = s.stack[newIdx];
        return { ...s, index: newIdx, skipping: true };
      });
      return target;
    },
  };
}
export const navHistory = createNavHistory();
export const canGoBack = derived(navHistory, $h => $h.index > 0);
export const canGoForward = derived(navHistory, $h => $h.index < $h.stack.length - 1);

// Derived
export const sortedNotes = derived(
  [notes, sortMode, viewMode, noteOrder],
  ([$notes, $sortMode, $viewMode, $noteOrder]) => {
    // Quick Access preserves stored order
    if ($viewMode === "quickaccess") return $notes;
    const customSortable =
      $viewMode === "all" || $viewMode === "notebook" || $viewMode === "tag";

    const pinned = $notes.filter((n) => n.meta.pinned);
    const unpinned = $notes.filter((n) => !n.meta.pinned);

    const sortFn = (a: NoteEntry, b: NoteEntry) => {
      switch ($sortMode) {
        case "custom": {
          if (!customSortable) {
            return (
              new Date(b.meta.modified).getTime() -
              new Date(a.meta.modified).getTime()
            );
          }
          const oa = $noteOrder[a.path] ?? Number.MAX_SAFE_INTEGER;
          const ob = $noteOrder[b.path] ?? Number.MAX_SAFE_INTEGER;
          if (oa !== ob) return oa - ob;
          return a.meta.title.localeCompare(b.meta.title);
        }
        case "title":
          return a.meta.title.localeCompare(b.meta.title);
        case "created":
          return (
            new Date(b.meta.created).getTime() -
            new Date(a.meta.created).getTime()
          );
        case "modified":
        default:
          return (
            new Date(b.meta.modified).getTime() -
            new Date(a.meta.modified).getTime()
          );
      }
    };

    return [...pinned.sort(sortFn), ...unpinned.sort(sortFn)];
  },
);

export const vaultState = derived(
  [
    activeNotePath,
    sidebarWidth,
    notelistWidth,
    sidebarCollapsed,
    collapsedNotebooks,
    notebookSortMode,
    notebookOrder,
    noteOrder,
  ],
  ([
    $activeNotePath,
    $sidebarWidth,
    $notelistWidth,
    $sidebarCollapsed,
    $collapsedNotebooks,
    $notebookSortMode,
    $notebookOrder,
    $noteOrder,
  ]) => {
    return {
      last_open_note: $activeNotePath,
      sidebar_width: $sidebarWidth,
      notelist_width: $notelistWidth,
      sidebar_collapsed: $sidebarCollapsed,
      collapsed_notebooks: $collapsedNotebooks,
      notebook_sort_mode: $notebookSortMode,
      notebook_order: $notebookOrder,
      note_order: $noteOrder,
    } satisfies VaultState;
  },
);
