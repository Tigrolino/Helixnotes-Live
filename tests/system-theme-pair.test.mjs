import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const appLayout = await readFile(
  new URL('../src/lib/components/AppLayout.svelte', import.meta.url),
  'utf8'
);

test('the theme shortcut recognizes a resolved custom dark theme', () => {
  assert.match(
    appLayout,
    /\$customThemes\.find\(theme => theme\.id === \$resolvedTheme\)/
  );
  assert.match(
    appLayout,
    /darkThemes\.includes\(\$resolvedTheme\) \|\| \(customTheme\?\.is_dark \?\? false\)/
  );
});

test('AppLayout leaves root theme application to the root layout', () => {
  assert.doesNotMatch(appLayout, /function applyTheme\(/);
  assert.doesNotMatch(appLayout, /applyTheme\(\$theme\)/);
});
