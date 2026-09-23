import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const source = await readFile(
  new URL('../src/lib/components/SettingsPanel.svelte', import.meta.url),
  'utf8'
);
const initialization = source.match(
  /(const\s+themePresets\s*=\s*\[[\s\S]*?\];)[\s\S]*?(?=\bconst\s+accentPresets\s*=)/
);
assert.ok(initialization, 'theme presets initialization was not found');

// Evaluate only the trusted repository declaration and its initialization code.
const unsorted = new Function(`${initialization[1]}\nreturn themePresets;`)();
const presets = new Function(`${initialization[0]}\nreturn themePresets;`)();

test('pins System, Light, and Dark in that exact order', () => {
  assert.deepEqual(presets.slice(0, 3).map(({ id, label }) => ({ id, label })), [
    { id: 'system', label: 'System' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' }
  ]);
});

test('orders remaining built-in labels alphabetically, including Rosé Pine', () => {
  const labels = presets.slice(3).map((preset) => preset.label);
  assert.ok(labels.includes('Rosé Pine'));
  assert.deepEqual(labels, [...labels].sort());
});

test('preserves every theme ID, label, and color without duplicates', () => {
  assert.equal(presets.length, unsorted.length);
  for (const themes of [unsorted, presets]) {
    assert.equal(new Set(themes.map((preset) => preset.id)).size, themes.length);
    assert.equal(new Set(themes.map((preset) => preset.label)).size, themes.length);
  }
  assert.deepEqual(
    Object.fromEntries(presets.map((preset) => [preset.id, preset])),
    Object.fromEntries(unsorted.map((preset) => [preset.id, preset]))
  );
});
