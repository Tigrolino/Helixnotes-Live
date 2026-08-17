import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const editor = await readFile(
  new URL('../src/lib/components/Editor.svelte', import.meta.url),
  'utf8'
);

test('Note Info owns the only open-note trash action', () => {
  assert.doesNotMatch(editor, /editor-trash-btn/);
  assert.match(editor, /class="info-trash-btn"[\s\S]{0,300}onclick=\{moveOpenNoteToTrash\}/);
  assert.match(editor, /class="info-trash-btn"[\s\S]{0,500}Move to Trash/);
  assert.equal(editor.match(/onclick=\{moveOpenNoteToTrash\}/g)?.length, 1);
});

test('Note Info hides the trash action when moving the note is unavailable', () => {
  assert.match(
    editor,
    /\{#if onMoveToTrash && \$viewMode !== 'trash'\}[\s\S]{0,200}class="info-section info-actions"/
  );
});

test('Note Info stays open when clicking another note', () => {
  assert.doesNotMatch(editor, /onInfoClickAway/);
  assert.match(editor, /class="info-close-btn" onclick=\{\(\) => showInfo = false\}/);
});
