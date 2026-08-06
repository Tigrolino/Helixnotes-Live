export interface NoteSwitcherNote {
	path: string;
	title: string;
	relativePath: string;
}

export interface NoteSwitcherRow {
	path: string;
	title: string;
	folder: string;
	current: boolean;
}

export interface NoteSwitcherSections {
	recent: NoteSwitcherRow[];
	quickAccess: NoteSwitcherRow[];
}

interface BuildNoteSwitcherSectionsOptions {
	currentPath: string | null;
	historyPaths: readonly string[];
	knownNotes: readonly NoteSwitcherNote[];
	quickAccessNotes: readonly NoteSwitcherNote[];
	recentLimit?: number;
}

function pathKey(path: string): string {
	return path.replace(/\\/g, '/');
}

function folderLabel(relativePath: string): string {
	const parts = relativePath.replace(/\\/g, '/').split('/').filter(Boolean);
	parts.pop();
	return parts.join('/') || 'Unfiled';
}

function toRow(note: NoteSwitcherNote, currentPathKey: string | null): NoteSwitcherRow {
	return {
		path: note.path,
		title: note.title,
		folder: folderLabel(note.relativePath),
		current: currentPathKey !== null && pathKey(note.path) === currentPathKey
	};
}

export function buildNoteSwitcherSections({
	currentPath,
	historyPaths,
	knownNotes,
	quickAccessNotes,
	recentLimit = 6
}: BuildNoteSwitcherSectionsOptions): NoteSwitcherSections {
	const knownByPath = new Map(knownNotes.map((note) => [pathKey(note.path), note]));
	const currentPathKey = currentPath ? pathKey(currentPath) : null;
	const recent: NoteSwitcherRow[] = [];
	const shownPaths = new Set<string>();
	const limit = Math.max(0, Math.floor(recentLimit));

	const addRecent = (path: string | null) => {
		if (!path || recent.length >= limit) return;
		const key = pathKey(path);
		if (shownPaths.has(key)) return;
		const note = knownByPath.get(key);
		if (!note) return;
		shownPaths.add(key);
		recent.push(toRow(note, currentPathKey));
	};

	addRecent(currentPath);
	for (let index = historyPaths.length - 1; index >= 0 && recent.length < limit; index -= 1) {
		addRecent(historyPaths[index]);
	}

	const quickAccess: NoteSwitcherRow[] = [];
	for (const note of quickAccessNotes) {
		const key = pathKey(note.path);
		if (shownPaths.has(key) || !knownByPath.has(key)) continue;
		shownPaths.add(key);
		quickAccess.push(toRow(note, currentPathKey));
	}

	return { recent, quickAccess };
}
