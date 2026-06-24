// Obsidian-compatible callouts.
//
// Callouts are written as blockquotes whose first line is `[!type]`:
//
//     > [!warning] Optional title
//     > Body markdown...
//
// A trailing `+` / `-` on the type makes the callout foldable
// (`+` = expanded, `-` = collapsed). Types are case-insensitive and have
// aliases. Callouts round-trip to this exact syntax so notes stay portable
// (no lock-in) and stay compatible with Obsidian.

// alias (lowercase) -> canonical group used for icon + colour
const ALIASES: Record<string, string> = {
	note: 'note',
	abstract: 'abstract', summary: 'abstract', tldr: 'abstract',
	info: 'info',
	todo: 'todo',
	tip: 'tip', hint: 'tip', important: 'tip',
	success: 'success', check: 'success', done: 'success',
	question: 'question', help: 'question', faq: 'question',
	warning: 'warning', caution: 'warning', attention: 'warning',
	failure: 'failure', fail: 'failure', missing: 'failure',
	danger: 'danger', error: 'danger',
	bug: 'bug',
	example: 'example',
	quote: 'quote', cite: 'quote',
};

/**
 * Resolve a (possibly aliased / cased) type to its canonical styling group.
 * Unknown / user-defined types resolve to 'custom' (neutral accent + tag icon)
 * so a `[!decision]` reads as intentional rather than masquerading as a note.
 */
export function calloutGroup(type: string): string {
	return ALIASES[(type || '').trim().toLowerCase()] || 'custom';
}

/** Default header label shown when no custom title is set (matches Obsidian). */
export function calloutLabel(type: string): string {
	const t = (type || 'note').trim();
	return t ? t.charAt(0).toUpperCase() + t.slice(1) : 'Note';
}

// Inner SVG markup per canonical group (24x24, stroke = currentColor).
const ICONS: Record<string, string> = {
	note: '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
	abstract: '<rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><path d="M9 12h6"/><path d="M9 16h6"/>',
	info: '<circle cx="12" cy="12" r="10"/><line x1="12" y1="11" x2="12" y2="16"/><line x1="12" y1="8" x2="12.01" y2="8"/>',
	todo: '<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>',
	tip: '<path d="M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.07-2.14-.22-4.05 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.15.43-2.29 1-3a2.5 2.5 0 0 0 2.5 2.5Z"/>',
	success: '<polyline points="20 6 9 17 4 12"/>',
	question: '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
	warning: '<path d="m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
	failure: '<circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/>',
	danger: '<polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/>',
	bug: '<rect x="8" y="6" width="8" height="14" rx="4"/><path d="m19 8-3 2"/><path d="m5 8 3 2"/><path d="m19 16-3-2"/><path d="m5 16 3-2"/><path d="M21 13h-5"/><path d="M3 13h5"/><path d="m15 4-1.5 2.5"/><path d="m9 4 1.5 2.5"/>',
	example: '<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>',
	quote: '<path d="M3 21c3 0 7-1 7-8V5c0-1.25-.76-2.02-2-2H4c-1.25 0-2 .75-2 1.97V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 0-1 1.03V20c0 1 0 1 1 1z"/><path d="M15 21c3 0 7-1 7-8V5c0-1.25-.76-2.02-2-2h-4c-1.25 0-2 .75-2 1.97V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1z"/>',
	custom: '<path d="M20.59 13.41 13.42 20.58a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/><line x1="7" y1="7" x2="7.01" y2="7"/>',
};

/** Full SVG icon string for a callout type, at the given pixel size. */
export function calloutIcon(type: string, size = 18): string {
	const inner = ICONS[calloutGroup(type)] || ICONS.note;
	return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${inner}</svg>`;
}

/** Ordered list of types offered in the type picker. */
export const CALLOUT_MENU: { type: string; label: string }[] = [
	{ type: 'note', label: 'Note' },
	{ type: 'abstract', label: 'Abstract' },
	{ type: 'info', label: 'Info' },
	{ type: 'todo', label: 'Todo' },
	{ type: 'tip', label: 'Tip' },
	{ type: 'success', label: 'Success' },
	{ type: 'question', label: 'Question' },
	{ type: 'warning', label: 'Warning' },
	{ type: 'failure', label: 'Failure' },
	{ type: 'danger', label: 'Danger' },
	{ type: 'bug', label: 'Bug' },
	{ type: 'example', label: 'Example' },
	{ type: 'quote', label: 'Quote' },
];

// First line of a callout: [!type], optional +/- fold marker, optional title.
const CALLOUT_RE = /^\s*\[!([\w-]+)\]([-+]?)[ \t]*(.*)$/;

const BLOCK_SELECTOR =
	'p,ul,ol,blockquote,pre,h1,h2,h3,h4,h5,h6,table,hr,div[data-callout],div[data-secret-block],div[data-math-block],div[data-pdf-src],div[data-page-break]';

/**
 * Convert callout blockquotes produced by markdown-it into
 * `<div data-callout=...>` blocks that the Callout node parses. Operates in
 * place on a detached container element. Processes depth-first so nested
 * callouts (`> > [!todo]`) are converted before their parents.
 */
export function transformCalloutBlockquotes(root: Element): void {
	const walk = (el: Element) => {
		for (const child of Array.from(el.children)) walk(child);
		if (el.tagName === 'BLOCKQUOTE') convertBlockquote(el);
	};
	walk(root);
}

function convertBlockquote(bq: Element): void {
	const first = bq.firstElementChild;
	if (!first || first.tagName !== 'P') return;

	// markdown-it merges the title line and any immediately-following body lines
	// into one paragraph (separated by "\n"). Split off the first line.
	const innerHtml = (first as HTMLElement).innerHTML;
	const nlIdx = innerHtml.indexOf('\n');
	const firstLineHtml = nlIdx === -1 ? innerHtml : innerHtml.slice(0, nlIdx);
	const restHtml = nlIdx === -1 ? '' : innerHtml.slice(nlIdx + 1);

	const probe = document.createElement('div');
	probe.innerHTML = firstLineHtml;
	const m = (probe.textContent || '').match(CALLOUT_RE);
	if (!m) return;

	const type = m[1].toLowerCase();
	const foldChar = m[2];
	const title = (m[3] || '').trim();
	const foldable = foldChar === '+' || foldChar === '-';
	const folded = foldChar === '-';

	const div = document.createElement('div');
	div.setAttribute('data-callout', type);
	div.setAttribute('data-callout-foldable', foldable ? 'true' : 'false');
	div.setAttribute('data-callout-folded', folded ? 'true' : 'false');
	if (title) div.setAttribute('data-callout-title', title);

	if (restHtml.trim()) {
		(first as HTMLElement).innerHTML = restHtml;
		div.appendChild(first);
	} else {
		first.remove();
	}
	while (bq.firstChild) div.appendChild(bq.firstChild);

	// content is `block+`; guarantee at least one block.
	if (!div.querySelector(BLOCK_SELECTOR)) div.appendChild(document.createElement('p'));

	bq.replaceWith(div);
}

/**
 * Serialize a Callout ProseMirror node back to `> [!type] ...` markdown.
 * `serializeChild` serializes a child block (the editor's own serializer),
 * which makes nesting fall out automatically (each `>` line gains another `>`).
 */
export function serializeCallout(node: any, serializeChild: (child: any) => string): string {
	const type = node.attrs.type || 'note';
	const foldable = !!node.attrs.foldable;
	const folded = !!node.attrs.folded;
	const title = (node.attrs.title || '').trim();
	const suffix = foldable ? (folded ? '-' : '+') : '';
	const header = `[!${type}]${suffix}${title ? ' ' + title : ''}`;

	const parts: string[] = [];
	node.forEach((child: any) => parts.push(serializeChild(child).replace(/\n+$/, '')));
	while (parts.length && parts[parts.length - 1] === '') parts.pop();

	const out = [`> ${header}`];
	const inner = parts.join('\n');
	if (inner.length) {
		for (const line of inner.split('\n')) out.push(line.length ? `> ${line}` : '>');
	}
	return out.join('\n') + '\n';
}
