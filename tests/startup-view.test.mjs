import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/utils/startup-view.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'startup-view.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const startup = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);

test('normalizes every supported default view and falls back to All Notes', () => {
  for (const view of ['all', 'quickaccess', 'tasks', 'daily']) {
    assert.equal(startup.normalizeStartupView(view), view);
  }
  assert.equal(startup.normalizeStartupView('notebook'), 'all');
  assert.equal(startup.normalizeStartupView(undefined), 'all');
});

test('uses the configured default when session restoration is disabled', () => {
  assert.deepEqual(startup.resolveStartupTarget({
    startupView: 'tasks',
    restoreLastSession: false,
    lastViewMode: 'daily',
    lastNotebook: null,
    lastTag: null
  }), { mode: 'tasks' });
});

test('restores a supported previous list when restoration is enabled', () => {
  assert.deepEqual(startup.resolveStartupTarget({
    startupView: 'daily',
    restoreLastSession: true,
    lastViewMode: 'quickaccess',
    lastNotebook: null,
    lastTag: null
  }), { mode: 'quickaccess' });
});

test('restores notebook and tag identifiers, otherwise uses the configured default', () => {
  assert.deepEqual(startup.resolveStartupTarget({
    startupView: 'all',
    restoreLastSession: true,
    lastViewMode: 'notebook',
    lastNotebook: 'Projects',
    lastTag: null
  }), { mode: 'notebook', notebookPath: 'Projects' });
  assert.deepEqual(startup.resolveStartupTarget({
    startupView: 'tasks',
    restoreLastSession: true,
    lastViewMode: 'tag',
    lastNotebook: null,
    lastTag: null
  }), { mode: 'tasks' });
});
