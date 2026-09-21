import assert from 'node:assert/strict';
import test from 'node:test';

const {
  applyCommittedTagStyle,
  createTagStylePersister,
  isValidTagColor,
  lookupTagStyle,
  nextTagStyle,
  normalizeTagStyleKey,
  tagColor,
  tagCustomImagePath,
  tagIterationKey,
  tagStyleKeyEquals
} = await import(new URL('./tag-styles.ts', import.meta.url));

test('normalizes tag style keys by trimming', () => {
  assert.equal(normalizeTagStyleKey('  work  '), 'work');
});

test('accepts 3 and 6 digit hex colors only', () => {
  assert.equal(isValidTagColor('#e11'), true);
  assert.equal(isValidTagColor('#e11d48'), true);
  assert.equal(isValidTagColor('#E11D48'), true);
  assert.equal(isValidTagColor('red'), false);
  assert.equal(isValidTagColor('#gg0000'), false);
  assert.equal(isValidTagColor('#e11d48aa'), false);
});

test('looks up tag styles with a case-insensitive fallback', () => {
  const styles = { Work: { icon: 'builtin:briefcase', color: '#e11d48' } };
  assert.equal(lookupTagStyle('Work', styles)?.icon, 'builtin:briefcase');
  assert.equal(lookupTagStyle('work', styles)?.color, '#e11d48');
  assert.equal(tagColor('missing', styles), null);
  assert.equal(tagStyleKeyEquals('Work', 'work'), true);
});

test('treats non-builtin icons as custom image paths', () => {
  const styles = { daily: { icon: '.helixnotes/attachments/tag.png' } };
  assert.equal(tagCustomImagePath('daily', styles), '.helixnotes/attachments/tag.png');
  assert.equal(tagCustomImagePath('daily', { daily: { icon: 'builtin:calendar' } }), null);
});

test('merges style patches and clears empty styles', () => {
  const current = { icon: 'builtin:star', color: '#3b82f6' };
  assert.deepEqual(nextTagStyle(current, { color: '#22c55e' }), {
    icon: 'builtin:star',
    color: '#22c55e'
  });
  assert.equal(nextTagStyle(current, { icon: null, color: null }), null);
});

test('resetting a differently cased tag removes the stored style', () => {
  const styles = { Work: { icon: 'builtin:briefcase', color: '#e11d48' } };
  assert.deepEqual(applyCommittedTagStyle(styles, 'work', null), {});
});

test('saving a style replaces a differently cased stored key', () => {
  const styles = { Work: { icon: 'builtin:briefcase' } };
  const next = applyCommittedTagStyle(styles, 'work', {
    icon: 'builtin:star',
    color: '#ec4899'
  });
  assert.deepEqual(next, {
    Work: { icon: 'builtin:star', color: '#ec4899' }
  });
  assert.equal(Object.hasOwn(next, 'work'), false);
});

test('queued icon then color patches keep both fields', async () => {
  const writes = [];
  let releaseFirst;
  const firstWrite = new Promise((resolve) => {
    releaseFirst = resolve;
  });

  const persister = createTagStylePersister({
    getCurrent: () => undefined,
    async save(next) {
      writes.push(next);
      if (writes.length === 1) await firstWrite;
    }
  });

  assert.deepEqual(persister.apply({ icon: 'builtin:star' }), { icon: 'builtin:star' });
  assert.deepEqual(persister.apply({ color: '#ec4899' }), {
    icon: 'builtin:star',
    color: '#ec4899'
  });

  releaseFirst();
  await persister.flush();

  assert.deepEqual(writes, [
    { icon: 'builtin:star' },
    { icon: 'builtin:star', color: '#ec4899' }
  ]);
});

test('queued reset after a pending save clears the stored style', async () => {
  let current = { icon: 'builtin:star' };
  const writes = [];
  let releaseFirst;
  const firstWrite = new Promise((resolve) => {
    releaseFirst = resolve;
  });

  const persister = createTagStylePersister({
    getCurrent: () => current,
    async save(next) {
      writes.push(next);
      current = next ?? undefined;
      if (writes.length === 1) await firstWrite;
    }
  });

  persister.apply({ color: '#8b5cf6' });
  assert.equal(persister.apply({ icon: null, color: null }), null);

  releaseFirst();
  await persister.flush();

  assert.deepEqual(writes, [
    { icon: 'builtin:star', color: '#8b5cf6' },
    null
  ]);
  assert.deepEqual(applyCommittedTagStyle({ work: writes[0] }, 'Work', writes[1]), {});
});

test('duplicate tag names still get unique each keys', () => {
  const tags = ['work', 'work'];
  const keys = tags.map((tag, index) => tagIterationKey(tag, index));
  assert.deepEqual(keys, ['0:work', '1:work']);
  assert.equal(new Set(keys).size, keys.length);
});
