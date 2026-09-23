export interface TagStyleFields {
  icon?: string | null;
  color?: string | null;
}

export const TAG_COLOR_PRESETS = [
  '#e11d48',
  '#f97316',
  '#eab308',
  '#22c55e',
  '#14b8a6',
  '#3b82f6',
  '#8b5cf6',
  '#ec4899',
  '#64748b'
];

const HEX_COLOR = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;
const BUILTIN_PREFIX = 'builtin:';

export function normalizeTagStyleKey(tag: string): string {
  return tag.trim();
}

export function isValidTagColor(value: string | null | undefined): value is string {
  return typeof value === 'string' && HEX_COLOR.test(value);
}

export function tagStyleKeyEquals(a: string, b: string): boolean {
  return normalizeTagStyleKey(a).toLowerCase() === normalizeTagStyleKey(b).toLowerCase();
}

export function existingTagStyleKey(
  tag: string,
  styles: Record<string, TagStyleFields>
): string | undefined {
  const key = normalizeTagStyleKey(tag);
  if (!key) return undefined;
  if (Object.hasOwn(styles, key)) return key;
  const lower = key.toLowerCase();
  return Object.keys(styles).find((name) => name.toLowerCase() === lower);
}

export function lookupTagStyle(
  tag: string,
  styles: Record<string, TagStyleFields>
): TagStyleFields | undefined {
  const storedKey = existingTagStyleKey(tag, styles);
  return storedKey ? styles[storedKey] : undefined;
}

/** Stable `{#each}` key when a note can contain duplicate tag names. */
export function tagIterationKey(tag: string, index: number): string {
  return `${index}:${tag}`;
}

export function tagColor(
  tag: string,
  styles: Record<string, TagStyleFields>
): string | null {
  const color = lookupTagStyle(tag, styles)?.color;
  return isValidTagColor(color) ? color : null;
}

export function tagCustomImagePath(
  tag: string,
  styles: Record<string, TagStyleFields>
): string | null {
  const icon = lookupTagStyle(tag, styles)?.icon;
  if (!icon || icon.startsWith(BUILTIN_PREFIX)) return null;
  return icon;
}

export function nextTagStyle(
  current: TagStyleFields | undefined,
  patch: TagStyleFields
): TagStyleFields | null {
  const icon = patch.icon === undefined ? current?.icon : patch.icon;
  const color = patch.color === undefined ? current?.color : patch.color;
  const next: TagStyleFields = {};
  if (typeof icon === 'string' && icon.trim()) next.icon = icon;
  if (isValidTagColor(color)) next.color = color;
  return next.icon || next.color ? next : null;
}

export function applyCommittedTagStyle(
  styles: Record<string, TagStyleFields>,
  tag: string,
  next: TagStyleFields | null
): Record<string, TagStyleFields> {
  const result = { ...styles };
  const key = normalizeTagStyleKey(tag);
  if (!key) return result;

  const lower = key.toLowerCase();
  for (const name of Object.keys(result)) {
    if (name.toLowerCase() === lower) delete result[name];
  }
  if (next) {
    const storedKey = existingTagStyleKey(tag, styles) ?? key;
    result[storedKey] = next;
  }
  return result;
}

export function createTagStylePersister(options: {
  getCurrent: () => TagStyleFields | undefined;
  save: (next: TagStyleFields | null) => Promise<void>;
}) {
  let draft: TagStyleFields | null | undefined;
  let queue = Promise.resolve();

  return {
    apply(patch: TagStyleFields): TagStyleFields | null {
      const current = draft === undefined ? options.getCurrent() : (draft ?? undefined);
      draft = nextTagStyle(current, patch);
      const snapshot = draft;
      const write = () => options.save(snapshot);
      queue = queue.then(write, write);
      return snapshot;
    },
    flush() {
      return queue;
    }
  };
}
