import type { Editor } from '@tiptap/core';
import type { Schema } from '@tiptap/pm/model';
import type { Transaction } from '@tiptap/pm/state';
import { liftTarget } from '@tiptap/pm/transform';

// Marks that carry content (URLs, note references) rather than visual
// formatting. Clearing formatting must never destroy these.
const CONTENT_MARKS: Record<string, true> = { link: true, wikiLink: true };

// Block types reset to plain paragraphs. Lists, callouts, collapsible
// sections and tables keep their structure so no state (checkboxes,
// nesting, cells) is lost.
const DEMOTE_BLOCKS: Record<string, true> = { heading: true, codeBlock: true };

// Outermost-first position of the first blockquote intersecting [from, to].
function findBlockquote(tr: Transaction, from: number, to: number): number | null {
	let found: number | null = null;
	tr.doc.nodesBetween(from, to, (node, pos) => {
		if (found !== null) return false;
		if (node.type.name === 'blockquote') {
			found = pos;
			return false;
		}
		return true;
	});
	return found;
}

/**
 * Strips inline formatting and demotes heading/code-block/blockquote
 * structure over the transaction's selection. An empty (cursor) selection
 * is expanded to the whole current text block, so a single invocation can
 * reset one messy line without highlighting it first.
 */
export function applyClearFormatting(schema: Schema, tr: Transaction): void {
	const { selection } = tr;
	const { $from } = selection;
	const expandToBlock = selection.empty && $from.parent.isTextblock;
	const from = expandToBlock ? $from.start() : selection.from;
	const to = expandToBlock ? $from.end() : selection.to;

	// 1. Strip formatting marks, keep content-bearing ones.
	for (const mark of Object.values(schema.marks)) {
		if (!CONTENT_MARKS[mark.name]) tr.removeMark(from, to, mark);
	}

	// 2. Demote headings and code blocks to paragraphs. Mark removal and
	//    retyping never move content, so the original positions stay valid.
	tr.doc.nodesBetween(from, to, (node, pos) => {
		if (DEMOTE_BLOCKS[node.type.name]) tr.setNodeMarkup(pos, schema.nodes.paragraph);
		return true;
	});

	// 3. Lift blockquote contents out of the quote wrapper, one pass per
	//    nesting level. Lifts move positions, so remap and rescan each pass.
	for (let guard = 0; guard < 20; guard++) {
		const quotePos = findBlockquote(tr, tr.mapping.map(from, 1), tr.mapping.map(to, -1));
		if (quotePos === null) break;
		// The range must span the blockquote's content: a collapsed
		// blockRange starts one depth up and wraps the quote itself.
		const quote = tr.doc.nodeAt(quotePos);
		if (!quote) break;
		const range = tr.doc.resolve(quotePos + 1).blockRange(tr.doc.resolve(quotePos + quote.nodeSize - 1));
		if (!range) break;
		const target = liftTarget(range);
		if (target === null) break;
		tr.lift(range, target);
	}
}

/** Clears formatting for the editor's current selection (or current line). */
export function clearFormatting(editor: Editor): boolean {
	return editor
		.chain()
		.focus()
		.command(({ state, tr }) => {
			applyClearFormatting(state.schema, tr);
			return true;
		})
		.run();
}
