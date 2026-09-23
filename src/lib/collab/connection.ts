// Singleton owner of the one collaboration WebSocket connection (the Rust core enforces there's
// only ever one anyway - see collab.rs's `CollabHandle` generation counter). Stage 2 had only
// SettingsPanel.svelte talking to it directly; Stage 3 adds a second listener (CollabTestDoc's
// Yjs binding), so this module exists to be the single place that owns the `Channel` passed to
// `connectCollab` and fans its events out to any number of subscribers.
//
// Usage:
//   import { connectCollabConnection, disconnectCollabConnection, refreshCollabStatus, onCollabData } from '$lib/collab/connection';
//   const unsubscribe = onCollabData((bytes) => { ... });
import {
  connectCollab,
  disconnectCollab,
  getCollabStatus,
  sendCollabData as sendCollabDataApi,
} from "$lib/api";
import { collabState } from "$lib/stores/app";
import type { CollabEvent } from "$lib/types";

type DataListener = (bytes: Uint8Array) => void;

const dataListeners = new Set<DataListener>();

function handleEvent(event: CollabEvent): void {
  switch (event.type) {
    case "status":
      collabState.set({ status: event.status, detail: event.detail });
      break;
    case "message":
      // Stage 2 debug/echo text frames; nothing in Stage 3 needs these, but keep them from
      // throwing if the server ever sends one (e.g. its own log/diagnostic frames).
      break;
    case "data": {
      const bytes = new Uint8Array(event.bytes);
      for (const listener of dataListeners) {
        try {
          listener(bytes);
        } catch (e) {
          console.error("Collaboration data listener threw:", e);
        }
      }
      break;
    }
  }
}

/** Open (or reopen) the collaboration connection. Safe to call again with new settings - the
 * Rust core stops any previous connection first, and this module keeps using the same fan-out
 * so already-registered `onCollabData` listeners keep working across a reconnect. */
export async function connectCollabConnection(
  url: string,
  workspace: string,
  password: string,
): Promise<void> {
  await connectCollab(url, workspace, password, handleEvent);
}

export async function disconnectCollabConnection(): Promise<void> {
  try {
    await disconnectCollab();
  } finally {
    collabState.set({ status: "disconnected", detail: null });
  }
}

/** Re-seed `collabState` from the Rust core's last known status - for a component that mounts
 * after the connection was already opened elsewhere (e.g. Settings panel reopened, or
 * CollabTestDoc mounting while already connected from Settings). Swallows errors: if no
 * connection has ever been attempted this session, or the backend isn't reachable yet, leave
 * `collabState` at its default ('disconnected') rather than surface an error from a passive
 * status check. */
export async function refreshCollabStatus(): Promise<void> {
  try {
    const snapshot = await getCollabStatus();
    collabState.set({ status: snapshot.status, detail: snapshot.detail });
  } catch {
    // See doc comment above.
  }
}

/** Send a raw binary frame over the active connection. Throws if not connected. */
export async function sendCollabData(bytes: Uint8Array): Promise<void> {
  await sendCollabDataApi(bytes);
}

/** Subscribe to incoming binary frames (Yjs sync/update messages). Returns an unsubscribe
 * function. Multiple subscribers are supported so more than one collaboration surface (e.g. a
 * future presence/awareness layer alongside CollabTestDoc's document sync) can share the one
 * connection. */
export function onCollabData(listener: DataListener): () => void {
  dataListeners.add(listener);
  return () => {
    dataListeners.delete(listener);
  };
}
