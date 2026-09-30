// Pure helpers for the AI-powered spell-check engine: building the prompt sent to the
// configured AI provider and parsing its response. Deliberately split out from Editor.svelte,
// which owns the ProseMirror-specific half (collecting numbered paragraphs from the live
// document, and turning parsed {block, word, suggestion} entries back into on-screen
// underlines) - this half has no DOM/editor dependency, so it can be unit tested directly.
//
// See commands.rs's "spell_check" ai_ask branch for the system prompt the model actually
// receives (it's told to reply with exactly the JSON shape parseSpellCheckResponse expects),
// and Editor.svelte's collectSpellTokens()/runAiSpellScan() for how this is wired up. The
// position-safety rule from the earlier Tab-corruption bug still applies here: this module
// only ever hands back a {block, word, suggestion} string triple - never a character offset -
// so the one piece of information the AI can't get right (exactly where in the live,
// possibly-already-edited document something is) is never the thing being trusted.

export interface SpellCheckPromptBlock {
	index: number;
	text: string;
}

export interface SpellCheckEntry {
	block: number;
	word: string;
	suggestion: string;
}

/** Builds the user message sent to the AI for spell-checking: one numbered line per paragraph,
 *  in exactly the "<number>: <paragraph text>" format the system prompt (in commands.rs) tells
 *  the model to expect. A block's text is a single ProseMirror textblock's concatenated text
 *  content, which can never itself contain a newline. */
export function buildSpellCheckPrompt(blocks: SpellCheckPromptBlock[]): string {
	return blocks.map((b) => `${b.index}: ${b.text}`).join('\n');
}

/** Parses the AI's response into a validated, de-duplicated list of corrections. Defensive
 *  about a model wrapping its JSON in a markdown code fence despite being told not to, and
 *  drops anything that doesn't match the expected shape rather than throwing - a malformed or
 *  unparseable response should just mean "no AI suggestions this round", never a crash or a
 *  scan that never completes. Entries are deduplicated per (block, lowercased word) - the last
 *  one wins if a model somehow repeats itself. */
export function parseSpellCheckResponse(raw: string): SpellCheckEntry[] {
	let text = raw.trim();
	const fenced = text.match(/^```(?:json)?\s*([\s\S]*?)\s*```$/);
	if (fenced) text = fenced[1].trim();
	if (!text) return [];

	let parsed: unknown;
	try {
		parsed = JSON.parse(text);
	} catch {
		return [];
	}
	if (!Array.isArray(parsed)) return [];

	const byKey = new Map<string, SpellCheckEntry>();
	for (const item of parsed) {
		if (!item || typeof item !== 'object') continue;
		const rec = item as Record<string, unknown>;
		const block = rec.block;
		const word = rec.word;
		const suggestion = rec.suggestion;
		if (typeof block !== 'number' || !Number.isFinite(block)) continue;
		if (typeof word !== 'string' || !word.trim()) continue;
		if (typeof suggestion !== 'string' || !suggestion.trim()) continue;
		const trimmedWord = word.trim();
		const trimmedSuggestion = suggestion.trim();
		if (trimmedWord.toLowerCase() === trimmedSuggestion.toLowerCase()) continue;
		const key = `${block}:${trimmedWord.toLowerCase()}`;
		byKey.set(key, { block, word: trimmedWord, suggestion: trimmedSuggestion });
	}
	return [...byKey.values()];
}
