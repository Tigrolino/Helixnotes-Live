import { TextSelection, type EditorState, type Transaction } from '@tiptap/pm/state';

export type WikiLinkAttributes = {
	title: string;
	path: string;
	aliased: boolean;
};

export function replaceWithWikiLink(
	state: EditorState,
	from: number,
	to: number,
	text: string,
	attrs: WikiLinkAttributes
): Transaction {
	const wikiLinkMark = state.schema.marks.wikiLink.create(attrs);
	const textNode = state.schema.text(text, [wikiLinkMark]);
	const transaction = state.tr.replaceWith(from, to, textNode);
	transaction.setSelection(TextSelection.create(transaction.doc, from + text.length));
	transaction.setStoredMarks([]);
	return transaction;
}
