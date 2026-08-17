import assert from 'node:assert/strict';
import test from 'node:test';
import { Schema } from '@tiptap/pm/model';
import { EditorState, Plugin, TextSelection } from '@tiptap/pm/state';
import { createCodeBlockInputScrollPlugin } from '../src/lib/editor/extensions/codeBlockInputScroll.ts';

const schema = new Schema({
	nodes: {
		doc: { content: 'block+' },
		paragraph: { content: 'text*', group: 'block' },
		codeBlock: {
			attrs: { language: { default: null } },
			content: 'text*',
			group: 'block',
			code: true,
		},
		text: { group: 'inline' },
	},
});

const inputRulesPlugin = new Plugin({ isInputRules: true });

function textBlock(type, text, attrs = null) {
	return schema.node(type, attrs, text ? [schema.text(text)] : undefined);
}

function stateWithSelection(type, text, options = {}) {
	const { attrs = null, from = text.length + 1, to = from } = options;
	const doc = schema.node('doc', null, [textBlock(type, text, attrs)]);

	return EditorState.create({
		doc,
		selection: TextSelection.create(doc, from, to),
		plugins: [inputRulesPlugin, createCodeBlockInputScrollPlugin()],
	});
}

function convertToCodeBlock(state, language = null, { inputRule = true } = {}) {
	const codeBlock = textBlock('codeBlock', '', { language });
	const tr = state.tr.replaceWith(0, state.doc.content.size, codeBlock);
	tr.setSelection(TextSelection.create(tr.doc, 1));
	if (inputRule) {
		tr.setMeta(inputRulesPlugin, {
			transform: tr,
			from: state.selection.from,
			to: state.selection.to,
			text: '\n',
		});
	}

	return state.applyTransaction(tr);
}

function assertAppendedScroll(result) {
	assert.equal(result.transactions.length, 2);
	assert.equal(result.transactions[0].scrolledIntoView, false);
	assert.equal(result.transactions[1].scrolledIntoView, true);
	assert.equal(result.state.selection.$from.parent.type.name, 'codeBlock');
}

function assertNoAppendedScroll(result) {
	assert.equal(result.transactions.length, 1);
	assert.equal(result.transactions[0].scrolledIntoView, false);
}

test('appends a scroll request for backtick and tilde input-rule transitions', () => {
	assertAppendedScroll(convertToCodeBlock(stateWithSelection('paragraph', '```')));
	assertAppendedScroll(convertToCodeBlock(stateWithSelection('paragraph', '~~~rust'), 'rust'));
});

test('does not append a scroll request for non-fence paragraph conversions', () => {
	assertNoAppendedScroll(convertToCodeBlock(stateWithSelection('paragraph', 'ordinary text')));
	assertNoAppendedScroll(convertToCodeBlock(stateWithSelection('paragraph', '```TypeScript')));
	assertNoAppendedScroll(convertToCodeBlock(stateWithSelection('paragraph', '````')));
});

test('does not append a scroll request for toolbar conversions or note loads', () => {
	assertNoAppendedScroll(
		convertToCodeBlock(stateWithSelection('paragraph', '```'), null, { inputRule: false })
	);
	assertNoAppendedScroll(
		convertToCodeBlock(stateWithSelection('paragraph', '~~~rust'), 'rust', { inputRule: false })
	);
});

test('does not append a scroll request for a non-empty fence selection', () => {
	const state = stateWithSelection('paragraph', '```', { from: 1, to: 4 });
	assertNoAppendedScroll(convertToCodeBlock(state));
});

test('does not append a scroll request for a transaction already in a code block', () => {
	const state = stateWithSelection('codeBlock', '```');
	const tr = state.tr.insertText('x');
	assertNoAppendedScroll(state.applyTransaction(tr));
});
