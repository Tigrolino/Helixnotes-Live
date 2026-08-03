import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import MarkdownIt from 'markdown-it';
import { Schema } from '@tiptap/pm/model';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('../src/lib/editor/markdown.ts', import.meta.url),
  'utf8'
);
const { code } = await transformWithEsbuild(source, 'markdown.ts', {
  loader: 'ts',
  format: 'esm',
  target: 'esnext'
});
const { serializeInlineMarkdown } = await import(
  `data:text/javascript;base64,${Buffer.from(code).toString('base64')}`
);

// Link intentionally precedes formatting marks in the schema. This matches the
// mark order produced by the app and reproduces the original per-text-node bug.
const schema = new Schema({
  nodes: {
    doc: { content: 'block+' },
    paragraph: { content: 'text*', group: 'block' },
    text: { group: 'inline' }
  },
  marks: {
    link: { attrs: { href: {} } },
    bold: {},
    italic: {},
    wikiLink: {
      attrs: {
        title: { default: null },
        aliased: { default: false }
      }
    }
  }
});

const markdown = new MarkdownIt({ html: true, linkify: false, breaks: false });
const bold = schema.marks.bold.create();
const italic = schema.marks.italic.create();
const link = schema.marks.link.create({ href: 'https://example.com' });

function paragraph(segments) {
  return schema.node(
    'paragraph',
    null,
    segments.map(({ text, marks }) => schema.text(text, marks))
  );
}

function assertMarkdown(segments, expected, expectedHtml) {
  const serialized = serializeInlineMarkdown(paragraph(segments));
  assert.equal(serialized, expected);
  assert.equal(markdown.render(serialized), `${expectedHtml}\n`);
}

test('keeps bold open when a link ends the marked span', () => {
  assertMarkdown(
    [
      { text: 'Bold ', marks: [bold] },
      { text: 'link', marks: [link, bold] }
    ],
    '**Bold [link](https://example.com)**',
    '<p><strong>Bold <a href="https://example.com">link</a></strong></p>'
  );
});

test('keeps bold open when a link starts the marked span', () => {
  assertMarkdown(
    [
      { text: 'link', marks: [link, bold] },
      { text: ' bold', marks: [bold] }
    ],
    '**[link](https://example.com) bold**',
    '<p><strong><a href="https://example.com">link</a> bold</strong></p>'
  );
});

test('keeps one bold span around a link in the middle', () => {
  assertMarkdown(
    [
      { text: 'Before ', marks: [bold] },
      { text: 'link', marks: [link, bold] },
      { text: ' after', marks: [bold] }
    ],
    '**Before [link](https://example.com) after**',
    '<p><strong>Before <a href="https://example.com">link</a> after</strong></p>'
  );
});

test('keeps nested bold and italic marks open across a link', () => {
  assertMarkdown(
    [
      { text: 'Nested ', marks: [bold, italic] },
      { text: 'link', marks: [link, bold, italic] },
      { text: ' marks', marks: [bold, italic] }
    ],
    '***Nested [link](https://example.com) marks***',
    '<p><em><strong>Nested <a href="https://example.com">link</a> marks</strong></em></p>'
  );
});

test('preserves wiki-link aliases inside a bold span', () => {
  const wikiLink = schema.marks.wikiLink.create({ title: 'Target note', aliased: true });
  const serialized = serializeInlineMarkdown(
    paragraph([
      { text: 'See ', marks: [bold] },
      { text: 'this note', marks: [wikiLink, bold] },
      { text: ' now', marks: [bold] }
    ])
  );

  assert.equal(serialized, '**See [[Target note|this note]] now**');
});
