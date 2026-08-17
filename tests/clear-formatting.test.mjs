import assert from 'node:assert/strict';
import test from 'node:test';
import { Schema } from '@tiptap/pm/model';
import { EditorState, TextSelection } from '@tiptap/pm/state';
import { applyClearFormatting } from '../src/lib/editor/clearFormatting.ts';

const schema = new Schema({
	nodes: {
		doc: { content: 'block+' },
		paragraph: { content: 'text*', group: 'block' },
		heading: { attrs: { level: { default: 1 } }, content: 'text*', group: 'block' },
		codeBlock: { content: 'text*', group: 'block', code: true },
		blockquote: { content: 'block+', group: 'block' },
		bulletList: { content: 'listItem+', group: 'block' },
		listItem: { content: 'paragraph block*' },
		taskList: { content: 'taskItem+', group: 'block' },
		taskItem: { attrs: { checked: { default: false } }, content: 'paragraph block*' },
		text: { group: 'inline' },
	},
	marks: {
		bold: {},
		italic: {},
		strike: {},
		underline: {},
		highlight: { attrs: { color: { default: null } } },
		code: {},
		link: { attrs: { href: {} } },
		wikiLink: { attrs: { path: { default: null } } },
	},
});

const mark = (name, attrs = null) => schema.mark(name, attrs);
const text = (value, marks = []) => schema.text(value, marks);
const para = (...children) => schema.node('paragraph', null, children);

function clearedDoc(doc, from, to = from) {
	const state = EditorState.create({ schema, doc, selection: TextSelection.create(doc, from, to) });
	const tr = state.tr;
	applyClearFormatting(schema, tr);
	return state.apply(tr).doc;
}

function markSummary(doc) {
	const summary = [];
	doc.descendants((node) => {
		if (node.isText) summary.push(`${node.text}→${node.marks.map((m) => m.type.name).join('+') || 'none'}`);
		return true;
	});
	return summary;
}

test('strips formatting marks from a selection but keeps link and wikiLink', () => {
	// "bold"[1,5) "link"[5,9) "wiki"[9,13)
	const doc = schema.node('doc', null, [
		para(
			text('bold', [mark('bold')]),
			text('link', [mark('link', { href: 'https://example.com' })]),
			text('wiki', [mark('wikiLink', { path: 'notes/wiki.md' })]),
		),
	]);

	const result = clearedDoc(doc, 1, 13);
	const nodes = [];
	result.firstChild.forEach((node) => nodes.push(node));

	assert.deepEqual(nodes[0].marks, []);
	assert.deepEqual(nodes[1].marks.map((m) => m.type.name), ['link']);
	assert.equal(nodes[1].marks[0].attrs.href, 'https://example.com');
	assert.deepEqual(nodes[2].marks.map((m) => m.type.name), ['wikiLink']);
	assert.equal(nodes[2].marks[0].attrs.path, 'notes/wiki.md');
});

test('empty selection clears the entire current line', () => {
	// "one "[1,5) "two"[5,8) " three"[8,14), cursor inside "one" at 2
	const doc = schema.node('doc', null, [
		para(text('one '), text('two', [mark('bold')]), text(' three', [mark('italic')])),
	]);

	const result = clearedDoc(doc, 2);
	assert.deepEqual(markSummary(result), ['one two three→none']);
});

test('cursor in a heading demotes it to a plain paragraph', () => {
	const doc = schema.node('doc', null, [
		schema.node('heading', { level: 2 }, [text('Title', [mark('bold')])]),
	]);

	const result = clearedDoc(doc, 3);
	assert.equal(result.firstChild.type.name, 'paragraph');
	assert.equal(result.firstChild.textContent, 'Title');
	assert.deepEqual(markSummary(result), ['Title→none']);
});

test('cursor in a code block converts it to a paragraph', () => {
	const doc = schema.node('doc', null, [schema.node('codeBlock', null, [text('const x = 1;')])]);

	const result = clearedDoc(doc, 4);
	assert.equal(result.firstChild.type.name, 'paragraph');
	assert.equal(result.firstChild.textContent, 'const x = 1;');
});

test('cursor in a blockquote lifts the content out of the wrapper', () => {
	const doc = schema.node('doc', null, [
		schema.node('blockquote', null, [para(text('quoted', [mark('italic')]))]),
	]);

	const result = clearedDoc(doc, 3);
	assert.equal(result.childCount, 1);
	assert.equal(result.firstChild.type.name, 'paragraph');
	assert.deepEqual(markSummary(result), ['quoted→none']);
});

test('nested blockquotes are fully unwrapped', () => {
	const doc = schema.node('doc', null, [
		schema.node('blockquote', null, [
			schema.node('blockquote', null, [para(text('deep', [mark('bold')]))]),
		]),
	]);

	const result = clearedDoc(doc, 3);
	assert.equal(result.childCount, 1);
	assert.equal(result.firstChild.type.name, 'paragraph');
	assert.deepEqual(markSummary(result), ['deep→none']);
});

test('task list structure and checked state survive clearing', () => {
	const doc = schema.node('doc', null, [
		schema.node('taskList', null, [
			schema.node('taskItem', { checked: true }, [para(text('done task', [mark('strike')]))]),
			schema.node('taskItem', { checked: false }, [para(text('open task'))]),
		]),
	]);
	// cursor inside "done task": taskList at 0, taskItem at 1, paragraph at 2, text from 3
	const result = clearedDoc(doc, 5);

	const list = result.firstChild;
	assert.equal(list.type.name, 'taskList');
	assert.equal(list.childCount, 2);
	assert.equal(list.firstChild.type.name, 'taskItem');
	assert.equal(list.firstChild.attrs.checked, true);
	assert.equal(list.lastChild.attrs.checked, false);
	assert.deepEqual(markSummary(result), ['done task→none', 'open task→none']);
});

test('partial selection keeps formatting outside the selection', () => {
	// "abcd" all bold; clear only [2,4) = "bc"
	const doc = schema.node('doc', null, [para(text('abcd', [mark('bold')]))]);

	const result = clearedDoc(doc, 2, 4);
	assert.deepEqual(markSummary(result), ['a→bold', 'bc→none', 'd→bold']);
});

test('multi-block selection demotes headings and strips marks in range', () => {
	// heading "H1" [0,4), paragraph at 4 with "body" bold [5,9)
	const doc = schema.node('doc', null, [
		schema.node('heading', { level: 1 }, [text('H1')]),
		para(text('body', [mark('bold')])),
	]);

	const result = clearedDoc(doc, 2, 7);
	assert.equal(result.firstChild.type.name, 'paragraph');
	// marks removed only up to position 7: "bo" cleared, "dy" still bold
	assert.deepEqual(markSummary(result), ['H1→none', 'bo→none', 'dy→bold']);
});
