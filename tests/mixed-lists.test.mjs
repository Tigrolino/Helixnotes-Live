import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { Schema } from '@tiptap/pm/model';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/editor/mixedLists.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'mixedLists.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const { convertListNode } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
);

const schema = new Schema({
  nodes: {
    doc: { content: 'block+' },
    paragraph: { content: 'text*', group: 'block' },
    text: { group: 'inline' },
    bulletList: { content: 'listItem+', group: 'block' },
    listItem: { content: 'paragraph block*' },
    taskList: { content: 'taskItem+', group: 'block' },
    taskItem: {
      attrs: { checked: { default: false } },
      content: 'paragraph block*'
    }
  }
});

function paragraph(text) {
  return schema.node('paragraph', null, [schema.text(text)]);
}

test('converts every task item in a list to a bullet item', () => {
  const sourceList = schema.node('taskList', null, [
    schema.node('taskItem', { checked: true }, [paragraph('First')]),
    schema.node('taskItem', { checked: false }, [paragraph('Second')])
  ]);

  const converted = convertListNode(schema, sourceList, 'bulletList');

  assert.equal(converted.type.name, 'bulletList');
  assert.deepEqual(
    converted.content.content.map((item) => item.type.name),
    ['listItem', 'listItem']
  );
  assert.equal(converted.textContent, 'FirstSecond');
  assert.equal(sourceList.type.name, 'taskList');
});

test('converts bullet items to unchecked tasks without dropping nested blocks', () => {
  const nestedBullets = schema.node('bulletList', null, [
    schema.node('listItem', null, [paragraph('Nested note')])
  ]);
  const sourceList = schema.node('bulletList', null, [
    schema.node('listItem', null, [paragraph('Parent'), nestedBullets])
  ]);

  const converted = convertListNode(schema, sourceList, 'taskList');
  const taskItem = converted.firstChild;

  assert.equal(converted.type.name, 'taskList');
  assert.equal(taskItem.type.name, 'taskItem');
  assert.equal(taskItem.attrs.checked, false);
  assert.equal(taskItem.lastChild.type.name, 'bulletList');
  assert.equal(taskItem.textContent, 'ParentNested note');
});

test('leaves a list alone when it already has the requested type', () => {
  const bullets = schema.node('bulletList', null, [
    schema.node('listItem', null, [paragraph('Unchanged')])
  ]);

  assert.equal(convertListNode(schema, bullets, 'bulletList'), null);
});
