import assert from 'node:assert/strict';
import test from 'node:test';

const { compareNaturalNames } = await import(
  new URL('../src/lib/utils/natural-sort.ts', import.meta.url)
);

test('sorts numeric name segments by numeric value', () => {
  const names = ['10g', '11g', '12g', '5g', '6g', '7g', '8g', '9g'];

  assert.deepEqual(names.sort(compareNaturalNames), [
    '5g',
    '6g',
    '7g',
    '8g',
    '9g',
    '10g',
    '11g',
    '12g'
  ]);
});

test('sorts numbers naturally wherever they appear in a name', () => {
  const names = ['Class 10b', 'Class 2b', 'Class 10a', 'Class 2a'];

  assert.deepEqual(names.sort(compareNaturalNames), [
    'Class 2a',
    'Class 2b',
    'Class 10a',
    'Class 10b'
  ]);
});
