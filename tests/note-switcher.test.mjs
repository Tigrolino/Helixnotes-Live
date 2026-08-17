import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/utils/note-switcher.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'note-switcher.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const { buildNoteSwitcherRequestPaths, buildNoteSwitcherSections } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
);

const note = (path, title, relativePath) => ({ path, title, relativePath });

test('requests the current note first, then unique history from newest to oldest', () => {
  const current = '/vault/Current.md';
  const recent = '/vault/Recent.md';
  const older = '/vault/Older.md';

  assert.deepEqual(
    buildNoteSwitcherRequestPaths(current, [older, recent, recent, current]),
    [current, recent, older]
  );
});

test('puts the current note first, then unique history from newest to oldest up to the recent limit', () => {
  const current = note('/vault/Current.md', 'Current', 'Current.md');
  const alpha = note('/vault/Projects/Alpha.md', 'Alpha', 'Projects/Alpha.md');
  const beta = note('/vault/Projects/Beta.md', 'Beta', 'Projects/Beta.md');

  const sections = buildNoteSwitcherSections({
    currentPath: current.path,
    historyPaths: [beta.path, alpha.path, alpha.path, current.path],
    knownNotes: [current, alpha, beta],
    quickAccessNotes: [],
    recentLimit: 3
  });

  assert.deepEqual(sections.recent, [
    { path: current.path, title: 'Current', folder: 'Unfiled', current: true },
    { path: alpha.path, title: 'Alpha', folder: 'Projects', current: false },
    { path: beta.path, title: 'Beta', folder: 'Projects', current: false }
  ]);
});

test('excludes stale, unknown, and external paths that are absent from the known-note inventory', () => {
  const current = note('/vault/Current.md', 'Current', 'Current.md');
  const known = note('/vault/Notes/Known.md', 'Known', 'Notes/Known.md');

  const sections = buildNoteSwitcherSections({
    currentPath: current.path,
    historyPaths: [
      '/vault/Deleted.md',
      known.path,
      '/home/user/Downloads/External.md'
    ],
    knownNotes: [current, known],
    quickAccessNotes: [
      note('/vault/Missing-Quick.md', 'Missing Quick', 'Missing-Quick.md'),
      note('/home/user/Downloads/Pinned.md', 'External Quick', 'Pinned.md')
    ]
  });

  assert.deepEqual(sections, {
    recent: [
      { path: current.path, title: 'Current', folder: 'Unfiled', current: true },
      { path: known.path, title: 'Known', folder: 'Notes', current: false }
    ],
    quickAccess: []
  });
});

test('keeps Quick Access order, removes Recent overlap, and derives root and nested folder labels', () => {
  const root = note('/vault/Root.md', 'Root', 'Root.md');
  const recentNested = note(
    '/vault/Projects/Helix/Plan.md',
    'Plan',
    'Projects/Helix/Plan.md'
  );
  const quickRoot = note('/vault/Scratch.md', 'Scratch', 'Scratch.md');
  const quickNested = note(
    '/vault/Areas/Reading/Queue.md',
    'Reading Queue',
    'Areas/Reading/Queue.md'
  );

  const sections = buildNoteSwitcherSections({
    currentPath: root.path,
    historyPaths: [recentNested.path],
    knownNotes: [root, recentNested, quickRoot, quickNested],
    quickAccessNotes: [quickRoot, root, quickNested, recentNested]
  });

  assert.deepEqual(sections, {
    recent: [
      { path: root.path, title: 'Root', folder: 'Unfiled', current: true },
      {
        path: recentNested.path,
        title: 'Plan',
        folder: 'Projects/Helix',
        current: false
      }
    ],
    quickAccess: [
      { path: quickRoot.path, title: 'Scratch', folder: 'Unfiled', current: false },
      {
        path: quickNested.path,
        title: 'Reading Queue',
        folder: 'Areas/Reading',
        current: false
      }
    ]
  });
});
