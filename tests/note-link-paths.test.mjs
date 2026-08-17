import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/utils/paths.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'paths.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const {
  assetSourceToMarkdown,
  assetUrlToLocalPath,
  normalizeLocalAssetPath,
  resolvePathFromFile,
  resolveVaultFilePath
} = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
);

test('resolves note links relative to Windows note paths', () => {
  assert.equal(
    resolvePathFromFile('C:\\Vault\\Current.md', './Target.md'),
    'C:/Vault/Target.md'
  );
  assert.equal(
    resolvePathFromFile('C:\\Vault\\Folder\\Current.md', '../Target.md'),
    'C:/Vault/Target.md'
  );
  assert.equal(
    resolvePathFromFile('\\\\server\\share\\Folder\\Current.md', '../Target.md'),
    '//server/share/Target.md'
  );
});

test('preserves Linux relative note-link resolution', () => {
  assert.equal(
    resolvePathFromFile('/vault/folder/Current.md', '../Target.md'),
    '/vault/Target.md'
  );
});

test('converts Windows asset URLs back to portable attachment paths', () => {
  assert.equal(
    normalizeLocalAssetPath('/C:\\Users\\user\\Vault\\image.png'),
    'C:/Users/user/Vault/image.png'
  );
  assert.equal(
    assetSourceToMarkdown(
      'http://asset.localhost/C%3A%5CUsers%5Cuser%5CVault%5C.helixnotes%5Cattachments%5Cimage.png',
      'C:\\Users\\user\\Vault\\Note.md',
      'C:\\Users\\user\\Vault'
    ),
    '.helixnotes/attachments/image.png'
  );
});

test('preserves portable asset paths on Linux and macOS', () => {
  assert.equal(
    assetSourceToMarkdown(
      'asset://localhost/%2Fhome%2Fuser%2FVault%2F.helixnotes%2Fattachments%2Fimage.png',
      '/home/user/Vault/Note.md',
      '/home/user/Vault'
    ),
    '.helixnotes/attachments/image.png'
  );
  assert.equal(
    assetSourceToMarkdown(
      'asset://localhost/%2FUsers%2Fuser%2FVault%2Fassets%2Fimage.png',
      '/Users/user/Vault/notes/Note.md',
      '/Users/user/Vault'
    ),
    '../assets/image.png'
  );
  assert.equal(
    assetSourceToMarkdown('../assets/image.png', '/vault/notes/Note.md', '/vault'),
    '../assets/image.png'
  );
});

test('decodes local asset URLs without damaging platform roots', () => {
  assert.equal(
    assetUrlToLocalPath('http://asset.localhost/C%3A%5CUsers%5Cuser%5CVault%5Cimage.png'),
    'C:/Users/user/Vault/image.png'
  );
  assert.equal(
    assetUrlToLocalPath('asset://localhost/%2Fhome%2Fuser%2FVault%2Fimage.png'),
    '/home/user/Vault/image.png'
  );
  assert.equal(
    assetUrlToLocalPath('http://asset.localhost/%5C%5Cserver%5Cshare%5CVault%5Cimage.png'),
    '//server/share/Vault/image.png'
  );
});

test('resolves vault files consistently across platforms', () => {
  assert.equal(
    resolveVaultFilePath('../assets/image.png', 'C:\\Vault\\notes\\Note.md', 'C:\\Vault'),
    'C:/Vault/assets/image.png'
  );
  assert.equal(
    resolveVaultFilePath('.helixnotes/attachments/image.png', 'C:\\Vault\\notes\\Note.md', 'C:\\Vault'),
    'C:/Vault/.helixnotes/attachments/image.png'
  );
  assert.equal(
    resolveVaultFilePath('../assets/image.png', '/vault/notes/Note.md', '/vault'),
    '/vault/assets/image.png'
  );
  assert.equal(
    resolveVaultFilePath('../assets/image.png', '\\\\server\\share\\Vault\\notes\\Note.md', '\\\\server\\share\\Vault'),
    '//server/share/Vault/assets/image.png'
  );
});
