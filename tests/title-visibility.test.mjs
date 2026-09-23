import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const { restoreTitleHeading, stripTitleHeading } = await import(
  new URL('../src/lib/editor/titleVisibility.ts', import.meta.url)
);
const editorSource = await readFile(
  new URL('../src/lib/components/Editor.svelte', import.meta.url),
  'utf8'
);

test('rich to source transition strips the restored title before display', () => {
  const start = editorSource.indexOf('if (isSource && !lastSourceMode) {');
  const end = editorSource.indexOf('} else if (!isSource && lastSourceMode)', start);

  assert.notEqual(start, -1, 'rich to source transition was not found');
  assert.notEqual(end, -1, 'rich to source transition boundary was not found');
  assert.match(
    editorSource.slice(start, end),
    /sourceContent\s*=\s*stripTitleH1\(\s*editor\s*\?\s*editorToMarkdown\(\)\s*:\s*\(\$activeNote\?\.content\s*\?\?\s*''\)\s*\)\s*;/
  );
});

test('source to rich transition preserves the hidden title on desktop and mobile', () => {
  const start = editorSource.indexOf('} else if (!isSource && lastSourceMode) {');
  const end = editorSource.indexOf('// Tauri drag-drop listener', start);
  const transition = editorSource.slice(start, end);

  assert.notEqual(start, -1, 'source to rich transition was not found');
  assert.notEqual(end, -1, 'source to rich transition boundary was not found');
  assert.match(
    transition,
    /editor\.commands\.setContent\(\s*markdownToHtml\(\s*restoreTitleH1\(\s*content\s*\)\s*\)\s*\)\s*;/
  );
  assert.match(transition, /createEditor\(\s*restoreTitleH1\(\s*content\s*\)\s*,\s*liveFieldId\s*\)\s*;/);
});

test('source to rich transition preserves an empty source body on desktop and mobile', () => {
  const start = editorSource.indexOf('} else if (!isSource && lastSourceMode) {');
  const end = editorSource.indexOf('// Tauri drag-drop listener', start);
  const transition = editorSource.slice(start, end);
  const contentAssignments = [...transition.matchAll(/const content = ([^;]+);/g)]
    .map((match) => match[1].trim());

  assert.deepEqual(contentAssignments, ['srcText', 'srcText']);
  assert.doesNotMatch(transition, /srcText\s*\|\|/);
});

test('keeps a hidden title through source to rich to source and save', () => {
  const persisted = '# Note title\n\n## Something else\n\nBody\n';
  const body = '## Something else\n\nBody\n';

  const initialSource = stripTitleHeading(persisted, 'Note title', true);
  const rich = stripTitleHeading(
    restoreTitleHeading(initialSource.markdown, initialSource.hiddenTitle),
    'Note title',
    true
  );
  const toggledSource = stripTitleHeading(
    restoreTitleHeading(rich.markdown, rich.hiddenTitle),
    'Note title',
    true
  );

  assert.equal(rich.markdown, body);
  assert.equal(toggledSource.markdown, body);
  assert.equal(restoreTitleHeading(toggledSource.markdown, toggledSource.hiddenTitle), persisted);
});

test('keeps a title-only note visually empty and saves one title heading', () => {
  const initialSource = stripTitleHeading('# Note title\n', 'Note title', true);
  const rich = stripTitleHeading(
    restoreTitleHeading(initialSource.markdown, initialSource.hiddenTitle),
    'Note title',
    true
  );
  const saved = restoreTitleHeading(rich.markdown, rich.hiddenTitle);

  assert.equal(initialSource.markdown, '');
  assert.equal(rich.markdown, '');
  assert.equal((saved.match(/^# Note title$/gm) ?? []).length, 1);
});

test('replaces hidden title state when an unrelated note loads', () => {
  const markdown = '## Something else\n\nOther body\n';
  const start = editorSource.indexOf('function stripTitleH1(md: string): string {');
  const end = editorSource.indexOf('function restoreTitleH1(md: string): string {', start);

  assert.deepEqual(stripTitleHeading(markdown, 'Another note', true), {
    markdown,
    hiddenTitle: null
  });
  assert.match(editorSource.slice(start, end), /hiddenTitleHeading\s*=\s*result\.hiddenTitle\s*;/);
});

test('leaves the title visible when title hiding is disabled', () => {
  const persisted = '# Note title\n\n## Something else\n';

  assert.deepEqual(stripTitleHeading(persisted, 'Note title', false), {
    markdown: persisted,
    hiddenTitle: null
  });
});
