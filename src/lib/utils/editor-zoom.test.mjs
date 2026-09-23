import assert from 'node:assert/strict';
import test from 'node:test';

const { getWheelFontSizeAction } = await import(
  new URL('./editor-zoom.ts', import.meta.url)
);

test('blocks modified wheel input when scroll font sizing is disabled', () => {
  assert.equal(
    getWheelFontSizeAction({ ctrlKey: false, metaKey: true, deltaY: -1 }, false),
    'block'
  );
  assert.equal(
    getWheelFontSizeAction({ ctrlKey: true, metaKey: false, deltaY: 1 }, false),
    'block'
  );
});

test('changes font size from modified wheel input when enabled', () => {
  assert.equal(
    getWheelFontSizeAction({ ctrlKey: false, metaKey: true, deltaY: -1 }, true),
    'increase'
  );
  assert.equal(
    getWheelFontSizeAction({ ctrlKey: true, metaKey: false, deltaY: 1 }, true),
    'decrease'
  );
});

test('ignores unmodified wheel input', () => {
  assert.equal(
    getWheelFontSizeAction({ ctrlKey: false, metaKey: false, deltaY: -1 }, false),
    'ignore'
  );
});
