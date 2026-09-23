import { getSelectionPair } from '../selectionPairs';

// Pure source-mode helper: computes the wrapped textarea value and preserved
// inner selection so Svelte event handlers stay as glue code.
export function wrapTextareaSelection(value: string, start: number, end: number, key: string) {
	const close = getSelectionPair(key);
	if (!close || start === end) return null;

	return {
		value: value.slice(0, start) + key + value.slice(start, end) + close + value.slice(end),
		selectionStart: start + key.length,
		selectionEnd: end + key.length,
	};
}
