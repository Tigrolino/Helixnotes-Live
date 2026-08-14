export const NOTEBOOK_ICON_OPTIONS = [
  { id: 'book', label: 'Book' },
  { id: 'folder', label: 'Folder' },
  { id: 'briefcase', label: 'Work' },
  { id: 'home', label: 'Home' },
  { id: 'star', label: 'Star' },
  { id: 'heart', label: 'Heart' },
  { id: 'lightbulb', label: 'Ideas' },
  { id: 'code', label: 'Code' },
  { id: 'flask', label: 'Research' },
  { id: 'graduation', label: 'Study' },
  { id: 'calendar', label: 'Calendar' },
  { id: 'tasks', label: 'Tasks' },
  { id: 'archive', label: 'Archive' },
  { id: 'globe', label: 'Globe' },
  { id: 'plane', label: 'Travel' },
  { id: 'palette', label: 'Creative' }
] as const;

export type NotebookIconId = (typeof NOTEBOOK_ICON_OPTIONS)[number]['id'];

const BUILTIN_PREFIX = 'builtin:';
const BUILTIN_IDS: Record<NotebookIconId, true> = {
  book: true,
  folder: true,
  briefcase: true,
  home: true,
  star: true,
  heart: true,
  lightbulb: true,
  code: true,
  flask: true,
  graduation: true,
  calendar: true,
  tasks: true,
  archive: true,
  globe: true,
  plane: true,
  palette: true
};

export function normalizeNotebookIconKey(path: string): string {
  return path.replace(/\\/g, '/');
}

export function encodeBuiltinNotebookIcon(icon: NotebookIconId): string {
  return `${BUILTIN_PREFIX}${icon}`;
}

export function decodeBuiltinNotebookIcon(value: string | null | undefined): NotebookIconId | null {
  if (!value?.startsWith(BUILTIN_PREFIX)) return null;
  const icon = value.slice(BUILTIN_PREFIX.length);
  return Object.hasOwn(BUILTIN_IDS, icon) ? icon as NotebookIconId : null;
}
