import { writable } from "svelte/store";

// A single key combination. `mod` means Ctrl on Windows/Linux and Cmd on macOS.
// `code` is a KeyboardEvent.code value (layout-independent), e.g. "KeyN",
// "Backslash", "ArrowLeft", "F11".
export interface Binding {
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
  code: string;
}

export interface ActionDef {
  id: string;
  label: string;
  group: "General" | "Interface" | "Navigation";
  default: Binding;
}

// Every customizable action and its factory-default binding. These mirror the
// handlers in AppLayout.handleKeydown and the rows in InfoPanel's Shortcuts tab.
export const ACTIONS: ActionDef[] = [
  { id: "new-note", label: "New note", group: "General", default: { mod: true, code: "KeyN" } },
  { id: "save", label: "Save", group: "General", default: { mod: true, code: "KeyS" } },
  { id: "quick-open", label: "Quick open", group: "General", default: { mod: true, code: "KeyP" } },
  { id: "find-in-note", label: "Find in note", group: "General", default: { mod: true, code: "KeyF" } },
  { id: "search-vault", label: "Search vault", group: "General", default: { mod: true, shift: true, code: "KeyF" } },
  { id: "open-new-window", label: "Open note in new window", group: "General", default: { mod: true, shift: true, code: "KeyW" } },

  { id: "toggle-sidebar", label: "Toggle sidebar", group: "Interface", default: { mod: true, code: "Backslash" } },
  { id: "toggle-theme", label: "Toggle dark / light theme", group: "Interface", default: { mod: true, shift: true, code: "KeyN" } },
  { id: "toggle-focus", label: "Toggle focus mode", group: "Interface", default: { mod: true, shift: true, code: "KeyE" } },
  { id: "toggle-readonly", label: "Toggle read-only", group: "Interface", default: { mod: true, shift: true, code: "KeyR" } },
  { id: "toggle-source", label: "Toggle source / WYSIWYG", group: "Interface", default: { mod: true, shift: true, code: "KeyM" } },
  { id: "fullscreen", label: "Toggle fullscreen", group: "Interface", default: { code: "F11" } },

  { id: "nav-back", label: "Go back", group: "Navigation", default: { alt: true, code: "ArrowLeft" } },
  { id: "nav-forward", label: "Go forward", group: "Navigation", default: { alt: true, code: "ArrowRight" } },
];

export const DEFAULT_BINDINGS: Record<string, Binding> = Object.fromEntries(
  ACTIONS.map((a) => [a.id, a.default]),
);

const STORAGE_KEY = "helix-keybindings";

function loadBindings(): Record<string, Binding> {
  const merged: Record<string, Binding> = { ...DEFAULT_BINDINGS };
  try {
    const raw = typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const stored = JSON.parse(raw) as Record<string, Binding>;
      for (const id of Object.keys(merged)) {
        if (stored[id]) merged[id] = stored[id];
      }
    }
  } catch {
    // Corrupt storage - fall back to defaults.
  }
  return merged;
}

export const keybindings = writable<Record<string, Binding>>(loadBindings());

keybindings.subscribe((map) => {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
    }
  } catch {
    // Ignore quota / unavailable storage.
  }
});

export function setBinding(id: string, binding: Binding) {
  keybindings.update((m) => ({ ...m, [id]: binding }));
}

export function resetBinding(id: string) {
  keybindings.update((m) => ({ ...m, [id]: DEFAULT_BINDINGS[id] }));
}

// True when the keyboard event matches the given binding exactly. Modifier
// state must match precisely so e.g. Mod+Shift+F never also triggers Mod+F.
export function matchBinding(e: KeyboardEvent, b: Binding | undefined): boolean {
  if (!b) return false;
  const mod = e.ctrlKey || e.metaKey;
  return (
    !!b.mod === mod &&
    !!b.shift === e.shiftKey &&
    !!b.alt === e.altKey &&
    e.code === b.code
  );
}

// Returns the id of the first action whose binding matches the event, or null.
export function matchAction(e: KeyboardEvent, map: Record<string, Binding>): string | null {
  for (const id of Object.keys(map)) {
    if (matchBinding(e, map[id])) return id;
  }
  return null;
}

// Build a Binding from a captured keydown event. Returns null for events that
// are only modifier keys (no real key pressed yet).
const MODIFIER_CODES = new Set([
  "ControlLeft", "ControlRight", "MetaLeft", "MetaRight",
  "ShiftLeft", "ShiftRight", "AltLeft", "AltRight",
]);

export function bindingFromEvent(e: KeyboardEvent): Binding | null {
  if (MODIFIER_CODES.has(e.code) || !e.code) return null;
  return {
    mod: e.ctrlKey || e.metaKey,
    shift: e.shiftKey,
    alt: e.altKey,
    code: e.code,
  };
}

export function bindingsEqual(a: Binding, b: Binding): boolean {
  return !!a.mod === !!b.mod && !!a.shift === !!b.shift && !!a.alt === !!b.alt && a.code === b.code;
}

const isMac = typeof navigator !== "undefined" && navigator.platform.startsWith("Mac");

const CODE_LABELS: Record<string, string> = {
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  Space: "Space",
  Enter: "Enter",
  Tab: "Tab",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
};

function codeToLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return code.slice(6);
  return CODE_LABELS[code] ?? code;
}

// Ordered list of key labels for rendering, e.g. ["Ctrl", "Shift", "N"].
export function bindingToKeys(b: Binding): string[] {
  const keys: string[] = [];
  if (b.mod) keys.push(isMac ? "⌘" : "Ctrl");
  if (b.alt) keys.push(isMac ? "⌥" : "Alt");
  if (b.shift) keys.push(isMac ? "⇧" : "Shift");
  keys.push(codeToLabel(b.code));
  return keys;
}
