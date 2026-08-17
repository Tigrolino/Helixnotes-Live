import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/utils/note-drag.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'note-drag.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const noteDrag = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);

test('round-trips every selected note path in a drag payload', () => {
  const paths = ['/vault/Alpha.md', '/vault/Projects/Beta.md'];
  assert.deepEqual(noteDrag.decodeNoteDragPaths(noteDrag.encodeNoteDragPaths(paths)), paths);
});

test('keeps single-note and Windows path payloads compatible', () => {
  assert.deepEqual(noteDrag.decodeNoteDragPaths('/vault/Alpha.md'), ['/vault/Alpha.md']);
  assert.deepEqual(
    noteDrag.decodeNoteDragPaths('C:\\Vault\\Alpha.md\r\nC:\\Vault\\Beta.md'),
    ['C:\\Vault\\Alpha.md', 'C:\\Vault\\Beta.md']
  );
});
