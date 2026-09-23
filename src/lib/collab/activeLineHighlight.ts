// A small custom TipTap extension: highlights the block another connected user is currently
// positioned in, using their assigned color. This is the "soft" editing-awareness indicator the
// collaboration plan calls for - purely visual, never blocking. It deliberately does NOT
// duplicate cursor tracking: `@tiptap/extension-collaboration-caret` (see CollabTestDoc.svelte)
// already maintains each remote user's cursor as a Yjs relative position in the shared
// `Awareness` instance's `cursor` field (via y-tiptap's yCursorPlugin). This extension just reads
// that same field and resolves it to "which block is that," reusing y-tiptap's own exported
// position-resolution helpers rather than re-deriving cursor tracking from scratch.
import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import type { EditorState } from '@tiptap/pm/state';
import * as Y from 'yjs';
import { ySyncPluginKey, relativePositionToAbsolutePosition, setMeta } from '@tiptap/y-tiptap';
import type { Awareness } from 'y-protocols/awareness';

export interface ActiveLineHighlightOptions {
	awareness: Awareness | null;
}

const pluginKey = new PluginKey('activeLineHighlight');

function buildDecorations(state: EditorState, awareness: Awareness | null): DecorationSet {
	if (!awareness) return DecorationSet.empty;
	const ystate = ySyncPluginKey.getState(state);
	if (!ystate?.doc || !ystate.binding) return DecorationSet.empty;

	const decorations: Decoration[] = [];
	awareness.getStates().forEach((clientState: Record<string, unknown>, clientId: number) => {
		if (clientId === awareness.clientID) return; // never highlight our own position
		const cursor = clientState?.cursor as { head?: unknown } | undefined;
		const user = clientState?.user as { name?: string; color?: string } | undefined;
		if (!cursor?.head || !user?.color) return;

		try {
			const relativeHead = Y.createRelativePositionFromJSON(cursor.head);
			const head = relativePositionToAbsolutePosition(
				ystate.doc,
				ystate.type,
				relativeHead,
				ystate.binding.mapping,
			);
			if (head == null) return;

			const clamped = Math.max(0, Math.min(head, state.doc.content.size));
			const $pos = state.doc.resolve(clamped);
			const depth = Math.max($pos.depth, 1);
			const blockStart = $pos.before(depth);
			const blockEnd = $pos.after(depth);

			decorations.push(
				Decoration.node(blockStart, blockEnd, {
					class: 'collab-active-line',
					style: `--collab-active-line-color: ${user.color}`,
					'data-collab-user': user.name ?? '',
				}),
			);
		} catch {
			// The relative position doesn't resolve against the current document (e.g. the block
			// was just deleted by someone else). Skip this user for this decoration pass - their
			// next awareness update will resolve against the new document shape.
		}
	});
	return DecorationSet.create(state.doc, decorations);
}

/**
 * Highlights the block each remote user's cursor currently resolves to. Requires `awareness` to
 * be the same `Awareness` instance passed to `CollaborationCaret`/`Collaboration` so the three
 * extensions agree on what "the document" and "the users" are.
 */
export const ActiveLineHighlight = Extension.create<ActiveLineHighlightOptions>({
	name: 'activeLineHighlight',

	addOptions() {
		return { awareness: null };
	},

	addProseMirrorPlugins() {
		const { awareness } = this.options;
		return [
			new Plugin({
				key: pluginKey,
				state: {
					init: (_, state) => buildDecorations(state, awareness),
					apply: (tr, previous, _oldState, newState) => {
						if (tr.getMeta(pluginKey)?.awarenessUpdated) {
							return buildDecorations(newState, awareness);
						}
						if (tr.docChanged) {
							return buildDecorations(newState, awareness);
						}
						return previous;
					},
				},
				props: {
					decorations: (state) => pluginKey.getState(state),
				},
				view: (view) => {
					if (!awareness) return {};
					// setMeta (from @tiptap/y-tiptap) batches same-tick awareness bursts into one
					// dispatched transaction - the same convention yCursorPlugin itself uses, reused
					// here rather than hand-rolling another debounce.
					const onAwarenessChange = () => setMeta(view, pluginKey, { awarenessUpdated: true });
					awareness.on('change', onAwarenessChange);
					return {
						destroy: () => awareness.off('change', onAwarenessChange),
					};
				},
			}),
		];
	},
});
