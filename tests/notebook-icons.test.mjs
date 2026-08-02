import assert from 'node:assert/strict';
import test from 'node:test';

const source = await import(new URL('../src/lib/utils/notebook-icons.ts', import.meta.url));

const {
  NOTEBOOK_ICON_OPTIONS,
  decodeBuiltinNotebookIcon,
  encodeBuiltinNotebookIcon
} = source;

test('built-in notebook icons have stable unique storage values', () => {
  assert.equal(NOTEBOOK_ICON_OPTIONS.length, 16);
  assert.equal(new Set(NOTEBOOK_ICON_OPTIONS.map(({ id }) => id)).size, NOTEBOOK_ICON_OPTIONS.length);

  for (const { id } of NOTEBOOK_ICON_OPTIONS) {
    const stored = encodeBuiltinNotebookIcon(id);
    assert.equal(stored, `builtin:${id}`);
    assert.equal(decodeBuiltinNotebookIcon(stored), id);
  }
});

test('custom paths and unknown built-in values remain outside the icon codec', () => {
  assert.equal(decodeBuiltinNotebookIcon('.helixnotes/attachments/notebook-icon.png'), null);
  assert.equal(decodeBuiltinNotebookIcon('builtin:unknown'), null);
  assert.equal(decodeBuiltinNotebookIcon(null), null);
});
