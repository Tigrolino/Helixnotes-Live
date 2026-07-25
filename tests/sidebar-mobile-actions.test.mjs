import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const sidebar = await readFile(
  new URL('../src/lib/components/Sidebar.svelte', import.meta.url),
  'utf8'
);

test('mobile notebook rows expose an accessible actions button', () => {
  assert.match(sidebar, /\{#if isMobile\}[\s\S]{0,500}class="notebook-actions-btn"/);
  assert.match(sidebar, /aria-label=\{`Actions for \$\{nb\.name\}`\}/);
  assert.match(sidebar, /onclick=\{\(e\) => openNotebookMenu\(e, nb\)\}/);
});

test('the notebook context menu receives its mobile styling outside the sidebar', () => {
  assert.match(sidebar, /class="context-menu" class:mobile=\{isMobile\}/);
  assert.match(sidebar, /\.context-menu\.mobile\s*\{/);
});

test('mobile action sheets stay inside every safe area and prevent tap-through', () => {
  assert.match(sidebar, /class="context-menu-backdrop"/);
  assert.match(sidebar, /safe-area-inset-left/);
  assert.match(sidebar, /safe-area-inset-right/);
  assert.match(sidebar, /safe-area-inset-top/);
  assert.match(sidebar, /safe-area-inset-bottom/);
});

test('the actions button remains inside the notebook manual-sort drop target', () => {
  assert.match(sidebar, /<div class="notebook-row" data-nb-path=\{nb\.path\}>/);
});
