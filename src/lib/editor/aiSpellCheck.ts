// Pure helpers for the AI-powered spell-check engine: building the prompt sent to the
// configured AI provider and parsing its response. Deliberately split out from Editor.svelte,
// which owns the ProseMirror-specific half (collecting numbered paragraphs from the live
// document, and turning parsed {block, word, suggestions} entries back into on-screen
// underlines) - this half has no DOM/editor dependency, so it can be unit tested directly.
//
// See commands.rs's "spell_check" ai_ask branch for the system prompt the model actually
// receives (it's told to reply with exactly the JSON shape parseSpellCheckResponse expects),
// and Editor.svelte's collectSpellTokens()/runAiSpellScan() for how this is wired up. The
// position-safety rule from the earlier Tab-corruption bug still applies here: this module
// only ever hands back a {block, word, suggestions} triple - never a character offset - so the
// one piece of information the AI can't get right (exactly where in the live, possibly-
// already-edited document something is) is never the thing being trusted.

export interface SpellCheckPromptBlock {
	index: number;
	text: string;
}

export interface SpellCheckEntry {
	block: number;
	word: string;
	// Up to 3, best guess first - mirrors the Basic engine's suggestCorrections(), so the
	// right-click menu can show several options under either engine. suggestions[0] is what
	// Tab-accept and the inline badge use, the same way suggestCorrection() (singular) is
	// suggestCorrections()[0] for the Basic engine.
	suggestions: string[];
}

const MAX_SUGGESTIONS = 3;

/** Builds the user message sent to the AI for spell-checking: one numbered line per paragraph,
 *  in exactly the "<number>: <paragraph text>" format the system prompt (in commands.rs) tells
 *  the model to expect. A block's text is a single ProseMirror textblock's concatenated text
 *  content, which can never itself contain a newline. */
export function buildSpellCheckPrompt(blocks: SpellCheckPromptBlock[]): string {
	return blocks.map((b) => `${b.index}: ${b.text}`).join('\n');
}

/** Recovers a usable array from JSON that's broken only at the very end - live testing against
 *  real (sometimes weaker/overloaded) models turned up a markdown fence that never closed, a
 *  stray extra character or bracket tacked on after the array's own closing `]`, and an array
 *  whose closing `]` just never arrived - each of which otherwise threw away an entire chunk's
 *  worth of real, correctly-found results over a few trailing characters. Finds the last `}` in
 *  the text (the close of the last complete entry, if the model got at least one full entry
 *  out), truncates there, and appends `]`. This only ever discards text AFTER the last complete
 *  entry it can find - it never touches or guesses at anything before that point, so it can't
 *  turn a genuinely corrupted entry (e.g. a stray `'` where a `"` belonged, in the middle of the
 *  array) into something it isn't. That case is correctly left to fail below, which loses just
 *  that one chunk's results rather than risking a misread word or suggestion. */
function tryRecoverTruncatedArray(text: string): unknown {
	// The last `}` in the text isn't necessarily the real close of the last complete entry -
	// there can be stray characters after it too (a dangling `"]}` was observed in practice,
	// which itself contains a `}`). Walk backward through every `}` position, trying each one in
	// turn, until one of them yields valid JSON once `]` is appended - capped well above any
	// realistic number of entries in one chunk, just so a pathological string can't spin forever.
	let searchFrom = text.length;
	for (let attempts = 0; attempts < 500; attempts++) {
		const brace = text.lastIndexOf('}', searchFrom - 1);
		if (brace === -1) return undefined;
		try {
			return JSON.parse(text.slice(0, brace + 1) + ']');
		} catch {
			searchFrom = brace;
		}
	}
	return undefined;
}

/** Parses the AI's response into a validated, de-duplicated list of corrections. Defensive
 *  about a model wrapping its JSON in a markdown code fence despite being told not to, and
 *  drops anything that doesn't match the expected shape rather than throwing - a malformed or
 *  unparseable response should just mean "no AI suggestions this round", never a crash or a
 *  scan that never completes. Entries are deduplicated per (block, lowercased word) - the last
 *  one wins if a model somehow repeats itself. */
export function parseSpellCheckResponse(raw: string): SpellCheckEntry[] {
	let text = raw.trim();
	// Strip a leading and/or trailing markdown fence independently, rather than requiring both
	// to be present as a single matched pair - a response can be cut off (a rate limit, a
	// connection hiccup, the model just stopping early) after opening a ```json fence but before
	// ever closing it, and that's just as recoverable as the missing-closing-`]` case
	// tryRecoverTruncatedArray() handles below.
	const openFence = text.match(/^```(?:json)?\s*/);
	if (openFence) text = text.slice(openFence[0].length);
	const closeFence = text.match(/\s*```$/);
	if (closeFence) text = text.slice(0, text.length - closeFence[0].length);
	text = text.trim();
	if (!text) return [];

	let parsed: unknown;
	try {
		parsed = JSON.parse(text);
	} catch {
		parsed = tryRecoverTruncatedArray(text);
		if (parsed === undefined) return [];
	}
	if (!Array.isArray(parsed)) return [];

	const byKey = new Map<string, SpellCheckEntry>();
	for (const item of parsed) {
		if (!item || typeof item !== 'object') continue;
		const rec = item as Record<string, unknown>;
		const block = rec.block;
		const word = rec.word;
		const rawSuggestions = rec.suggestions;
		if (typeof block !== 'number' || !Number.isFinite(block)) continue;
		if (typeof word !== 'string' || !word.trim()) continue;
		if (!Array.isArray(rawSuggestions)) continue;
		const trimmedWord = word.trim();
		const lowerWord = trimmedWord.toLowerCase();

		const seen = new Set<string>();
		const suggestions: string[] = [];
		for (const s of rawSuggestions) {
			if (typeof s !== 'string' || !s.trim()) continue;
			const trimmed = s.trim();
			const lower = trimmed.toLowerCase();
			if (lower === lowerWord || seen.has(lower)) continue;
			seen.add(lower);
			suggestions.push(trimmed);
			if (suggestions.length >= MAX_SUGGESTIONS) break;
		}
		if (!suggestions.length) continue;

		const key = `${block}:${lowerWord}`;
		byKey.set(key, { block, word: trimmedWord, suggestions });
	}
	return [...byKey.values()];
}
