import type { Mark, Node as ProseMirrorNode } from '@tiptap/pm/model';

type AssetSourceNormalizer = (source: string) => string;

type MarkdownMark = {
  key: string;
  priority: number;
  open: string;
  close: string;
};

const identityAssetSource: AssetSourceNormalizer = (source) => source;

function stringAttr(mark: Mark, name: string): string {
  const value = mark.attrs?.[name];
  return typeof value === 'string' ? value : '';
}

function markdownMark(mark: Mark, text: string): MarkdownMark | null {
  switch (mark.type.name) {
    case 'bold':
      return { key: 'bold', priority: 10, open: '**', close: '**' };
    case 'italic':
      return { key: 'italic', priority: 20, open: '*', close: '*' };
    case 'strike':
      return { key: 'strike', priority: 30, open: '~~', close: '~~' };
    case 'underline':
      return { key: 'underline', priority: 40, open: '<u>', close: '</u>' };
    case 'subscript':
      return { key: 'subscript', priority: 50, open: '~', close: '~' };
    case 'superscript':
      return { key: 'superscript', priority: 60, open: '^', close: '^' };
    case 'highlight': {
      const color = stringAttr(mark, 'color');
      return color
        ? {
            key: `highlight:${color}`,
            priority: 70,
            open: `<mark data-color="${color}">`,
            close: '</mark>',
          }
        : { key: 'highlight', priority: 70, open: '==', close: '==' };
    }
    case 'textStyle': {
      const color = stringAttr(mark, 'color');
      return color
        ? {
            key: `textStyle:${color}`,
            priority: 80,
            open: `<span style="color: ${color}">`,
            close: '</span>',
          }
        : null;
    }
    case 'code':
      return { key: 'code', priority: 90, open: '`', close: '`' };
    case 'link': {
      const href = stringAttr(mark, 'href');
      return {
        key: `link:${href}`,
        priority: 100,
        open: '[',
        close: `](${href})`,
      };
    }
    case 'wikiLink': {
      const title = stringAttr(mark, 'title') || text;
      const aliased = mark.attrs?.aliased === true || title !== text;
      return {
        key: `wikiLink:${title}:${aliased}`,
        priority: 110,
        open: aliased ? `[[${title}|` : '[[',
        close: ']]',
      };
    }
    default:
      return null;
  }
}

function marksForText(node: ProseMirrorNode): MarkdownMark[] {
  return node.marks
    .map((mark) => markdownMark(mark, node.text || ''))
    .filter((mark): mark is MarkdownMark => mark !== null)
    .sort((left, right) => left.priority - right.priority || left.key.localeCompare(right.key));
}

function commonMarkCount(active: MarkdownMark[], next: MarkdownMark[]): number {
  const length = Math.min(active.length, next.length);
  let index = 0;
  while (index < length && active[index].key === next[index].key) index += 1;
  return index;
}

export function serializeInlineMarkdown(
  node: ProseMirrorNode,
  normalizeAssetSource: AssetSourceNormalizer = identityAssetSource,
): string {
  let result = '';
  let activeMarks: MarkdownMark[] = [];

  const transitionMarks = (nextMarks: MarkdownMark[]) => {
    const shared = commonMarkCount(activeMarks, nextMarks);
    for (let index = activeMarks.length - 1; index >= shared; index -= 1) {
      result += activeMarks[index].close;
    }
    for (let index = shared; index < nextMarks.length; index += 1) {
      result += nextMarks[index].open;
    }
    activeMarks = nextMarks;
  };

  node.forEach((child, _offset, index) => {
    if (child.isText) {
      transitionMarks(marksForText(child));
      let text = child.text || '';
      if (index === 0) {
        text = text.replace(/^[\t\u2003]+/, (whitespace) => '&emsp;'.repeat(whitespace.length));
      }
      result += text;
      return;
    }

    transitionMarks([]);

    if (child.type.name === 'image') {
      const source = normalizeAssetSource(child.attrs.src || '');
      if (!source) return;
      const alt = child.attrs.alt || '';
      const size = child.attrs['data-size'] || child.attrs.size || 'full';
      const sizeSuffix = size && size !== 'full' ? `|size=${size}` : '';
      if (result) result += '\n';
      result += `![${alt}${sizeSuffix}](${source})`;
    } else if (child.type.name === 'mathInline') {
      result += `$${child.attrs.tex || ''}$`;
    } else if (child.type.name === 'hardBreak') {
      result += '  \n';
    }
  });

  transitionMarks([]);
  return result;
}
