// Shared source of truth for editor behaviors that wrap an active selection.
// Keep this small: only characters users reasonably expect to form pairs.
const selectionPairs: Record<string, string> = {
	'(': ')',
	'{': '}',
	'[': ']',
	"'": "'",
	'"': '"',
	'`': '`',
};

// Returns the closing character for a typed wrapper key, or null to pass through.
export function getSelectionPair(key: string) {
	return selectionPairs[key] ?? null;
}
