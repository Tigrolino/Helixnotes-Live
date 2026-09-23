// Minimal Yjs sync protocol, hand-rolled because the relay server (collab-server) is
// intentionally "dumb" - it broadcasts opaque binary frames between clients in the same
// workspace and does not understand Yjs at all. This module is the only place that
// understands the wire format; it mirrors the well-known y-protocols/sync message shapes
// (sync step 1 = state vector, sync step 2 = diff, update = incremental change) but is
// reimplemented directly against lib0/encoding + lib0/decoding and yjs so we don't pull in
// the full y-protocols/y-websocket dependency tree for three message types.
//
// Stage 4 adds a fourth, generic message type for awareness (presence/cursors/selections/
// typing - see CollabTestDoc.svelte). This module stays decoupled from y-protocols/awareness
// itself - it just wraps/unwraps an opaque awareness payload in the same envelope as the doc
// messages above, so one binary frame stream (and one "dumb" relay) carries both kinds of
// traffic without the relay ever needing to tell them apart.
import * as Y from 'yjs';
import * as encoding from 'lib0/encoding';
import * as decoding from 'lib0/decoding';

export const MESSAGE_SYNC_STEP1 = 0;
export const MESSAGE_SYNC_STEP2 = 1;
export const MESSAGE_UPDATE = 2;
export const MESSAGE_AWARENESS = 3;

export type SendFn = (message: Uint8Array) => void;

/** Step 1: "here is my state vector, send me what I'm missing." Sent on connect. */
export function encodeSyncStep1(doc: Y.Doc): Uint8Array {
	const encoder = encoding.createEncoder();
	encoding.writeVarUint(encoder, MESSAGE_SYNC_STEP1);
	encoding.writeVarUint8Array(encoder, Y.encodeStateVector(doc));
	return encoding.toUint8Array(encoder);
}

/** Step 2: "here is the diff you're missing," sent in reply to a peer's step 1. */
function encodeSyncStep2(diff: Uint8Array): Uint8Array {
	const encoder = encoding.createEncoder();
	encoding.writeVarUint(encoder, MESSAGE_SYNC_STEP2);
	encoding.writeVarUint8Array(encoder, diff);
	return encoding.toUint8Array(encoder);
}

/** An incremental update to broadcast after a local edit. */
export function encodeUpdateMessage(update: Uint8Array): Uint8Array {
	const encoder = encoding.createEncoder();
	encoding.writeVarUint(encoder, MESSAGE_UPDATE);
	encoding.writeVarUint8Array(encoder, update);
	return encoding.toUint8Array(encoder);
}

/**
 * Wrap an already-encoded awareness update (from `y-protocols/awareness`'s
 * `encodeAwarenessUpdate`) in this protocol's envelope. The payload itself is opaque here - see
 * src/lib/collab/CollabTestDoc.svelte, the only place that encodes/decodes what's inside it.
 */
export function encodeAwarenessMessage(awarenessUpdate: Uint8Array): Uint8Array {
	const encoder = encoding.createEncoder();
	encoding.writeVarUint(encoder, MESSAGE_AWARENESS);
	encoding.writeVarUint8Array(encoder, awarenessUpdate);
	return encoding.toUint8Array(encoder);
}

/**
 * Decode and apply one incoming protocol message to `doc`. `send` is used to reply to a
 * SyncStep1 with our own SyncStep2 (the standard two-way handshake). All updates applied to
 * `doc` here are tagged with origin `'remote'` so local-update listeners (which broadcast
 * outbound) can ignore them and we don't create an echo loop.
 *
 * `onAwareness`, if given, receives the raw payload of a MESSAGE_AWARENESS frame unwrapped from
 * its envelope - this module does not decode it further (that needs an `Awareness` instance,
 * which lives with the caller, not here). Omit it and awareness frames are silently ignored,
 * same as any other unrecognized message type.
 */
export function applyIncomingMessage(
	doc: Y.Doc,
	message: Uint8Array,
	send: SendFn,
	onAwareness?: (payload: Uint8Array) => void,
	onSyncStep1Reply?: () => void,
): void {
	const decoder = decoding.createDecoder(message);
	const messageType = decoding.readVarUint(decoder);
	switch (messageType) {
		case MESSAGE_SYNC_STEP1: {
			const remoteStateVector = decoding.readVarUint8Array(decoder);
			const diff = Y.encodeStateAsUpdate(doc, remoteStateVector);
			send(encodeSyncStep2(diff));
			// The peer that sent this just joined (or rejoined) and has no way to know about
			// awareness state we broadcast before they connected - the relay doesn't replay
			// anything, so tell them about us again now rather than waiting for our next change.
			onSyncStep1Reply?.();
			break;
		}
		case MESSAGE_SYNC_STEP2:
		case MESSAGE_UPDATE: {
			const update = decoding.readVarUint8Array(decoder);
			Y.applyUpdate(doc, update, 'remote');
			break;
		}
		case MESSAGE_AWARENESS: {
			const payload = decoding.readVarUint8Array(decoder);
			onAwareness?.(payload);
			break;
		}
		default:
			// Unknown message type: ignore rather than throw, so a future protocol addition
			// doesn't break older clients.
			break;
	}
}

/**
 * Subscribe to local edits on `doc` and forward each one as an UPDATE message via `send`,
 * skipping updates that originated remotely (origin === 'remote') to avoid echoing a peer's
 * own change back at it. Returns an unsubscribe function.
 */
export function subscribeLocalUpdates(doc: Y.Doc, send: SendFn): () => void {
	const handler = (update: Uint8Array, origin: unknown) => {
		if (origin === 'remote') return;
		send(encodeUpdateMessage(update));
	};
	doc.on('update', handler);
	return () => doc.off('update', handler);
}
