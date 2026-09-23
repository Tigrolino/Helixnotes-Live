// The "Live Notebook": a single collaborative notebook (files + sub-notebooks, all live-edited)
// that behaves like any other local notebook in the sidebar, except its contents are backed by a
// shared Yjs document synced over the same relay Stage 2/3 built, instead of local vault files.
//
// Design: ONE Y.Doc per configured collaboration workspace holds everything -
//   - `doc.getMap('tree')`: fileId -> {id, name, type, parentId, order} - the notebook/file tree,
//     a CRDT map so concurrent creates/renames/deletes/moves merge safely with no coordination.
//   - `doc.getXmlFragment(fileId)`: each file's own rich-text content, as a separately-named
//     shared type within the SAME doc (this is exactly what @tiptap/extension-collaboration's
//     `field` option is for - see Editor.svelte's createEditor()).
// Routing multiple documents this way - as named shared types inside one Y.Doc - means the relay
// server needs NO changes at all: it already broadcasts arbitrary binary frames within a
// workspace (see collab-server's README), and one workspace now just carries a richer Y.Doc
// instead of a single flat text document.
//
// Lifecycle mirrors connection.ts/testDocState.ts's established pattern: created lazily on first
// use, lives for as long as the collaboration connection stays open, and resets only on an
// explicit disconnect - not on navigating away from the live notebook in the UI. There is
// deliberately no persistence beyond that yet (no GitHub backing store exists yet - that's a
// later stage); if every client disconnects, the notebook's content only survives if at least one
// of them reconnects before the relay itself restarts, same limitation the single test document
// always had.
import { get, writable } from "svelte/store";
import * as Y from "yjs";
import * as awarenessProtocol from "y-protocols/awareness";
import { onCollabData, sendCollabData } from "./connection";
import {
  encodeSyncStep1,
  encodeAwarenessMessage,
  applyIncomingMessage,
  subscribeLocalUpdates,
} from "./syncProtocol";
import { appConfig, activeVaultConfig, collabState } from "$lib/stores/app";

export interface LiveTreeEntry {
  id: string;
  name: string;
  type: "file" | "folder";
  parentId: string | null;
  order: number;
  /** Absent/undefined means "not pinned" - kept optional rather than defaulted to `false` so
   * entries created before this field existed don't need a migration. */
  pinned?: boolean;
}

export interface LivePresenceEntry {
  clientId: number;
  name: string;
  color: string;
  /** The live file (fragment id) this person currently has open, or null if they're connected
   * but not looking at any file right now - see setLocalOpenFile(). */
  openFile: string | null;
}

const PRESENCE_COLORS = [
  "#ef4444",
  "#f97316",
  "#eab308",
  "#22c55e",
  "#14b8a6",
  "#3b82f6",
  "#8b5cf6",
  "#ec4899",
];
function colorForClientId(clientId: number): string {
  const idx =
    ((clientId % PRESENCE_COLORS.length) + PRESENCE_COLORS.length) % PRESENCE_COLORS.length;
  return PRESENCE_COLORS[idx];
}
function randomGuestName(): string {
  return `Guest ${Math.floor(1000 + Math.random() * 9000)}`;
}

interface SharedLiveNotebook {
  doc: Y.Doc;
  awareness: awarenessProtocol.Awareness;
  tree: Y.Map<LiveTreeEntry>;
  localName: string;
  localColor: string;
  send: (message: Uint8Array) => void;
  broadcastLocalAwareness: () => void;
  stopRemoteData: () => void;
  stopLocalUpdates: () => void;
  stopAwarenessBroadcast: () => void;
}

let shared: SharedLiveNotebook | null = null;
let unsubscribeCollabState: (() => void) | null = null;

/** The live notebook's file/sub-notebook tree, kept in sync with the shared Y.Map. Empty when
 * not connected. */
export const liveTreeEntries = writable<LiveTreeEntry[]>([]);

/** Everyone else currently connected to the live notebook (never includes the local client) -
 * this is connection-scoped, not tied to which file (if any) someone has open, so it reflects
 * "who's here" the moment you're connected, not just "who's editing the same file as you". */
export const livePresence = writable<LivePresenceEntry[]>([]);

function refreshTreeStore() {
  liveTreeEntries.set(shared ? Array.from(shared.tree.values()) : []);
}

function refreshPresenceStore() {
  if (!shared) {
    livePresence.set([]);
    return;
  }
  const list: LivePresenceEntry[] = [];
  shared.awareness.getStates().forEach((state: Record<string, unknown>, clientId: number) => {
    if (clientId === shared!.awareness.clientID) return;
    const user = state.user as { name?: string; color?: string } | undefined;
    if (!user) return;
    list.push({
      clientId,
      name: typeof user.name === "string" && user.name ? user.name : "Guest",
      color: typeof user.color === "string" ? user.color : "#999999",
      openFile: typeof state.openFile === "string" ? (state.openFile as string) : null,
    });
  });
  livePresence.set(list);
}

/** Announce (or clear) which live file the local user currently has open, so everyone else's
 * livePresence list can show "N people viewing this file" - called from Editor.svelte's
 * createEditor()/destroyEditor() as the bound live field changes. A no-op when not connected
 * (nothing to announce to). */
export function setLocalOpenFile(fieldId: string | null): void {
  if (!shared) return;
  shared.awareness.setLocalStateField("openFile", fieldId);
}

function create(): SharedLiveNotebook {
  const doc = new Y.Doc();
  const awareness = new awarenessProtocol.Awareness(doc);
  const tree = doc.getMap<LiveTreeEntry>("tree");

  const vc = activeVaultConfig(get(appConfig));
  const localName = vc?.collab_display_name?.trim() || randomGuestName();
  const localColor = colorForClientId(doc.clientID);

  const send = (message: Uint8Array) => {
    sendCollabData(message).catch((e) => console.error("Failed to send collaboration data:", e));
  };

  /** Re-announce our own full awareness state (not just a delta) - used on first connect, and
   * again whenever a peer's SyncStep1 tells us they just joined and won't have seen any of our
   * earlier per-change broadcasts (the relay has no memory/replay, so a client that joined after
   * we last changed something only learns about us if we tell it again). */
  const broadcastLocalAwareness = () => {
    send(encodeAwarenessMessage(awarenessProtocol.encodeAwarenessUpdate(awareness, [awareness.clientID])));
  };

  // Broadcast every local awareness change (user info, live cursor/selection from
  // CollaborationCaret, and setLocalOpenFile()) to peers. Without this, awareness state only ever
  // updates the local Awareness instance and never reaches anyone else - see broadcastLocalAwareness
  // above for why a plain per-change broadcast alone still isn't enough for late joiners.
  const onAwarenessUpdate = (
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ) => {
    if (origin === "remote") return;
    const changed = added.concat(updated, removed);
    if (changed.length === 0) return;
    send(encodeAwarenessMessage(awarenessProtocol.encodeAwarenessUpdate(awareness, changed)));
  };
  awareness.on("update", onAwarenessUpdate);

  // Publish presence as soon as we're in the live notebook, independent of whether any file is
  // open yet - "who's here" shouldn't require having something open, unlike a file's live cursor.
  // (Registered after onAwarenessUpdate above so this initial set is itself broadcast.)
  awareness.setLocalStateField("user", { name: localName, color: localColor });

  const stopRemoteData = onCollabData((bytes) => {
    applyIncomingMessage(
      doc,
      bytes,
      send,
      (payload) => {
        awarenessProtocol.applyAwarenessUpdate(awareness, payload, "remote");
      },
      // A peer just sent us SyncStep1 (they're (re)joining) - tell them about us right away
      // rather than waiting for our next awareness change, which might be minutes away or never.
      broadcastLocalAwareness,
    );
  });
  const stopLocalUpdates = subscribeLocalUpdates(doc, send);
  const stopAwarenessBroadcast = () => awareness.off("update", onAwarenessUpdate);

  // Announce our (empty, at creation time) state vector so an already-connected peer replies
  // with a SyncStep2 diff containing the whole notebook - tree and every file's content - as it
  // currently stands, and announce our presence so they learn about us too.
  send(encodeSyncStep1(doc));
  broadcastLocalAwareness();

  return {
    doc,
    awareness,
    tree,
    localName,
    localColor,
    send,
    broadcastLocalAwareness,
    stopRemoteData,
    stopLocalUpdates,
    stopAwarenessBroadcast,
  };
}

/** The shared live notebook, created on first use and reused for as long as the connection stays
 * open. Also lazily wires up presence/tree reactivity and the reset-on-disconnect watcher. */
export function getLiveNotebook(): SharedLiveNotebook {
  if (!shared) {
    shared = create();
    shared.tree.observe(refreshTreeStore);
    shared.awareness.on("change", refreshPresenceStore);
    refreshTreeStore();
    refreshPresenceStore();
    if (!unsubscribeCollabState) {
      let first = true;
      let wasConnected = false;
      unsubscribeCollabState = collabState.subscribe((s) => {
        if (first) {
          first = false;
          wasConnected = s.status === "connected";
          return;
        }
        if (s.status === "disconnected") {
          wasConnected = false;
          resetLiveNotebook();
          return;
        }
        if (s.status === "connected") {
          if (!wasConnected && shared) {
            // We just (re)connected after a drop - a heartbeat timeout, a network blip, etc.
            // The dumb relay has no memory of frames sent while we were gone, so re-run the join
            // handshake from scratch to pick up anything we missed instead of silently drifting
            // out of sync until the next full page reload.
            shared.send(encodeSyncStep1(shared.doc));
            shared.broadcastLocalAwareness();
          }
          wasConnected = true;
        }
      });
    }
  }
  return shared;
}

/** Tear down the shared live notebook. Called when the collaboration connection itself is
 * explicitly disconnected - not when the UI merely navigates away from the live notebook view. */
export function resetLiveNotebook(): void {
  if (!shared) return;
  const s = shared;
  shared = null;
  awarenessProtocol.removeAwarenessStates(s.awareness, [s.awareness.clientID], "disconnected");
  s.tree.unobserve(refreshTreeStore);
  s.awareness.off("change", refreshPresenceStore);
  s.stopAwarenessBroadcast();
  s.stopLocalUpdates();
  s.stopRemoteData();
  s.awareness.destroy();
  s.doc.destroy();
  liveTreeEntries.set([]);
  livePresence.set([]);
}

function newId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/** Create a new file in the live notebook and return its id. */
export function createLiveFile(name: string, parentId: string | null): string {
  const { tree } = getLiveNotebook();
  const id = newId();
  tree.set(id, { id, name, type: "file", parentId, order: Date.now() });
  return id;
}

/** Create a new sub-notebook (folder) in the live notebook and return its id. */
export function createLiveFolder(name: string, parentId: string | null): string {
  const { tree } = getLiveNotebook();
  const id = newId();
  tree.set(id, { id, name, type: "folder", parentId, order: Date.now() });
  return id;
}

/** Rename a file or sub-notebook. */
export function renameLiveEntry(id: string, name: string): void {
  const { tree } = getLiveNotebook();
  const entry = tree.get(id);
  if (!entry) return;
  tree.set(id, { ...entry, name });
}

/** Pin or unpin a file. Mirrors renameLiveEntry/moveLiveEntry's read-modify-write pattern - a
 * plain field replacement on the shared Y.Map entry, which merges safely with concurrent edits
 * to other fields the same way a rename or move does. */
export function setLiveEntryPinned(id: string, pinned: boolean): void {
  const { tree } = getLiveNotebook();
  const entry = tree.get(id);
  if (!entry) return;
  tree.set(id, { ...entry, pinned });
}

/** Move a file or sub-notebook to a new parent (null = notebook root). */
export function moveLiveEntry(id: string, parentId: string | null): void {
  const { tree } = getLiveNotebook();
  const entry = tree.get(id);
  if (!entry) return;
  tree.set(id, { ...entry, parentId });
}

/** Delete a file, or a sub-notebook and everything inside it (recursively). Anyone connected can
 * delete anything, by design - there's no per-file ownership in this feature. */
export function deleteLiveEntry(id: string): void {
  const { tree, doc } = getLiveNotebook();
  const entry = tree.get(id);
  if (!entry) return;
  if (entry.type === "folder") {
    for (const child of Array.from(tree.values())) {
      if (child.parentId === id) deleteLiveEntry(child.id);
    }
  } else {
    const fragment = doc.getXmlFragment(id);
    if (fragment.length > 0) fragment.delete(0, fragment.length);
  }
  tree.delete(id);
}

// ── Integration with the real notebook/note infrastructure ──
//
// The Live Notebook is presented as an ordinary NotebookEntry that sits in the sidebar's
// "Notebooks" tree right next to local notebooks, and its files are presented as ordinary
// NoteEntry/NoteContent objects, so NoteList.svelte, Sidebar.svelte and Editor.svelte need no
// live-specific branching of their own - they already handle any NotebookEntry/NoteEntry the
// same way regardless of where it came from. The only place that needs to know the difference is
// api.ts, which routes a call to one of the functions below instead of invoke() whenever the
// path it was given belongs to the live notebook - see isLiveNotebookPath()/isLiveNotePath().
//
// Path scheme: every live tree entry (file or folder) gets the synthetic path
// `${LIVE_NOTEBOOK_PATH}/${entry.id}` - flat and id-based rather than name-based, since names can
// collide or change (concurrent renames) while ids are stable CRDT keys. The root notebook itself
// is just `LIVE_NOTEBOOK_PATH`.
import type { NoteEntry, NoteContent, NoteMeta, NotebookEntry } from "$lib/types";

export const LIVE_NOTEBOOK_PATH = "__live__";
export const LIVE_NOTEBOOK_NAME = "Live Notebook";

export function isLiveNotebookPath(path: string | null | undefined): boolean {
  return !!path && (path === LIVE_NOTEBOOK_PATH || path.startsWith(LIVE_NOTEBOOK_PATH + "/"));
}

// Files and folders share the same id-space and the same path scheme, so this is really the same
// check as isLiveNotebookPath() - kept as a separate name for readability at call sites that deal
// with notes specifically.
export const isLiveNotePath = isLiveNotebookPath;

function liveEntryId(path: string): string | null {
  if (path === LIVE_NOTEBOOK_PATH) return null;
  if (!path.startsWith(LIVE_NOTEBOOK_PATH + "/")) return null;
  return path.slice(LIVE_NOTEBOOK_PATH.length + 1);
}

function entryToNoteMeta(entry: LiveTreeEntry): NoteMeta {
  // `order` doubles as the entry's creation timestamp (see createLiveFile/createLiveFolder) -
  // there's no separate "last modified" tracked yet (content changes happen straight in the
  // Yjs fragment, with no save event to hook), so both fields use it for now rather than
  // leaving them blank, which rendered as "Invalid Date" in the note list.
  const created = new Date(entry.order).toISOString();
  return { id: entry.id, title: entry.name, tags: [], pinned: !!entry.pinned, created, modified: created };
}

function entryToNoteEntry(entry: LiveTreeEntry): NoteEntry {
  const path = `${LIVE_NOTEBOOK_PATH}/${entry.id}`;
  return { path, relative_path: path, meta: entryToNoteMeta(entry), preview: "" };
}

function buildNotebookChildren(entries: LiveTreeEntry[], parentId: string | null): NotebookEntry[] {
  return entries
    .filter((e) => e.type === "folder" && e.parentId === parentId)
    .sort((a, b) => a.order - b.order)
    .map((folder) => ({
      name: folder.name,
      path: `${LIVE_NOTEBOOK_PATH}/${folder.id}`,
      relative_path: `${LIVE_NOTEBOOK_PATH}/${folder.id}`,
      children: buildNotebookChildren(entries, folder.id),
      note_count: entries.filter((e) => e.type === "file" && e.parentId === folder.id).length,
    }));
}

/** Build the Live Notebook's NotebookEntry tree from the current tree entries - same shape a
 * real local notebook has, so it can be rendered by the exact same sidebar/list code. */
export function buildLiveNotebookEntry(entries: LiveTreeEntry[]): NotebookEntry {
  return {
    name: LIVE_NOTEBOOK_NAME,
    path: LIVE_NOTEBOOK_PATH,
    relative_path: LIVE_NOTEBOOK_PATH,
    children: buildNotebookChildren(entries, null),
    note_count: entries.filter((e) => e.type === "file" && e.parentId === null).length,
  };
}

/** List the files directly inside a live notebook/folder path (mirrors api.ts's getNotes). */
export function getLiveNotes(notebookPath: string): NoteEntry[] {
  const { tree } = getLiveNotebook();
  const parentId = liveEntryId(notebookPath);
  return Array.from(tree.values())
    .filter((e) => e.type === "file" && e.parentId === parentId)
    .sort((a, b) => a.order - b.order)
    .map(entryToNoteEntry);
}

/** Mirrors api.ts's readNote. The body itself isn't meaningful here - Editor.svelte binds the
 * TipTap Collaboration extension directly to the file's Y.XmlFragment instead of using this
 * content string, the same way it would ignore stale content for any note it's about to bind
 * live updates to. */
export function readLiveNote(path: string): NoteContent {
  const { tree } = getLiveNotebook();
  const id = liveEntryId(path);
  const entry = id ? tree.get(id) : undefined;
  const meta = entry
    ? entryToNoteMeta(entry)
    : { id: id ?? "", title: "Untitled", tags: [], pinned: false, created: new Date().toISOString(), modified: new Date().toISOString() };
  return { path, meta, content: "", raw: "" };
}

/** Mirrors api.ts's saveNote. Body content is already live via the Yjs binding, so the only
 * things a "save" can mean here are the title and pinned state changing (both are plain
 * tree-entry fields, written through the same field-merge pattern as a rename). Tags aren't
 * supported on live notes yet - entryToNoteMeta always reports an empty list - so meta.tags is
 * intentionally not persisted here. */
export function saveLiveNote(path: string, meta: NoteMeta): void {
  const id = liveEntryId(path);
  if (!id) return;
  const { tree } = getLiveNotebook();
  const entry = tree.get(id);
  if (!entry) return;
  tree.set(id, { ...entry, name: meta.title, pinned: meta.pinned });
}

/** Mirrors api.ts's createNote. */
export function createLiveNote(notebookRelative: string | null, title: string): NoteEntry {
  const parentId = notebookRelative ? liveEntryId(notebookRelative) : null;
  const id = createLiveFile(title, parentId);
  const { tree } = getLiveNotebook();
  return entryToNoteEntry(tree.get(id)!);
}

/** Mirrors api.ts's duplicateNote. Content isn't cloned (no simple id-preserving way to copy a
 * Y.XmlFragment's contents into a fresh one) - the duplicate starts as a new empty file, same as
 * "New file" would, which is preferable to silently failing or crashing. */
export function duplicateLiveNote(path: string): NoteEntry {
  const id = liveEntryId(path);
  const { tree } = getLiveNotebook();
  const entry = id ? tree.get(id) : undefined;
  if (!entry) throw new Error("Live note not found");
  const newId = createLiveFile(`${entry.name} copy`, entry.parentId);
  return entryToNoteEntry(tree.get(newId)!);
}

/** Mirrors api.ts's renameNote/renameNotebook. The path never changes (it's id-based, not
 * name-based), unlike a real note whose path is its filename. */
export function renameLiveNoteOrNotebook(path: string, newName: string): string {
  const id = liveEntryId(path);
  if (id) renameLiveEntry(id, newName);
  return path;
}

/** Mirrors api.ts's deleteNote/deleteNotebook. */
export function deleteLiveNoteOrNotebook(path: string): void {
  const id = liveEntryId(path);
  if (id) deleteLiveEntry(id);
}

/** Mirrors api.ts's moveNote/moveNotebook. destNotebookPath must itself be a live path - moving
 * across the local/live boundary isn't supported (there's no vault-relative equivalent of a
 * shared Yjs fragment to move it to). */
export function moveLiveNoteOrNotebook(path: string, destNotebookPath: string): string {
  const id = liveEntryId(path);
  const destId = liveEntryId(destNotebookPath);
  if (id) moveLiveEntry(id, destId);
  return path;
}

/** Mirrors api.ts's createNotebook. */
export function createLiveSubNotebook(parentRelative: string | null, name: string): NotebookEntry {
  const parentId = parentRelative ? liveEntryId(parentRelative) : null;
  const id = createLiveFolder(name, parentId);
  const path = `${LIVE_NOTEBOOK_PATH}/${id}`;
  return { name, path, relative_path: path, children: [], note_count: 0 };
}

/** The Yjs fragment field id for a live note's path, or null if the path doesn't belong to the
 * Live Notebook. Used by Editor.svelte to decide whether (and to which field) to bind the TipTap
 * Collaboration extension. */
export function liveFieldIdForPath(path: string | null | undefined): string | null {
  if (!path || !path.startsWith(LIVE_NOTEBOOK_PATH + "/")) return null;
  return path.slice(LIVE_NOTEBOOK_PATH.length + 1);
}
