// Offline spell checker backing the "Spelling corrections" feature's Basic engine: nspell, a
// pure-JS Hunspell-compatible implementation, checked against the bundled Hunspell en_US
// dictionary (static/dictionaries/en.aff + en.dic - see that folder's README for
// provenance/license) - the same dictionary format and engine family that power spell-check in
// Word, Chrome, Firefox, and LibreOffice. No AI call, no network access after the one-time
// local asset fetch, and works with no AI provider configured at all - unlike ghost-text (and
// the AI spell-check engine in aiSpellCheck.ts), this has nothing to do with
// $appConfig.ai_provider.
//
// nspell's own suggest() ranks candidates using Hunspell's internal heuristics (keyboard
// adjacency, phonetic similarity, affix-aware edit distance, etc.), which is usually solid but
// can bury an extremely common correction behind several obscure dictionary words that happen
// to be an equally "close" edit - raw suggest('teh') puts "ten"/"eh"/"meh"/"tea" ahead of "the",
// for example, and raw suggest('adn') puts "an" ahead of "and". rerankSuggestions() below
// re-sorts nspell's own candidate list - it never adds or invents a candidate nspell didn't
// already surface - to put a common word first and, among ties, prefer whichever edit pattern
// (transposition/substitution/deletion/insertion) an everyday typo is more likely to be.

import nspell from 'nspell';

// A couple hundred of the most frequent words in English (articles, pronouns, prepositions,
// conjunctions, and other everyday function/content words) - used only to break ties when more
// than one of nspell's own candidates is a plausible correction (see rerankSuggestions() below).
// nspell/Hunspell has no frequency data of its own, so without this, a tie between a very
// common word and an obscure one (e.g. "teh" is one transposition away from both "the" and the
// far rarer "ten"/"tea"/"tel") is broken by nspell's internal heuristics alone, which don't
// account for which candidate a person is actually more likely to have meant.
const COMMON_WORDS = new Set([
	'the', 'of', 'and', 'a', 'to', 'in', 'is', 'you', 'that', 'it', 'he', 'was', 'for', 'on',
	'are', 'as', 'with', 'his', 'they', 'i', 'at', 'be', 'this', 'have', 'from', 'or', 'one',
	'had', 'by', 'word', 'but', 'not', 'what', 'all', 'were', 'we', 'when', 'your', 'can',
	'said', 'there', 'use', 'an', 'each', 'which', 'she', 'do', 'how', 'their', 'if', 'will',
	'up', 'other', 'about', 'out', 'many', 'then', 'them', 'these', 'so', 'some', 'her',
	'would', 'make', 'like', 'him', 'into', 'time', 'has', 'look', 'two', 'more', 'write',
	'go', 'see', 'number', 'no', 'way', 'could', 'people', 'my', 'than', 'first', 'water',
	'been', 'call', 'who', 'its', 'now', 'find', 'long', 'down', 'day', 'did', 'get', 'come',
	'made', 'may', 'part', 'over', 'new', 'sound', 'take', 'only', 'little', 'work', 'know',
	'place', 'year', 'live', 'me', 'back', 'give', 'most', 'very', 'after', 'thing', 'our',
	'just', 'name', 'good', 'sentence', 'man', 'think', 'say', 'great', 'where', 'help',
	'through', 'much', 'before', 'line', 'right', 'too', 'mean', 'old', 'any', 'same', 'tell',
	'boy', 'follow', 'came', 'want', 'show', 'also', 'around', 'form', 'three', 'small', 'set',
	'put', 'end', 'does', 'another', 'well', 'large', 'must', 'big', 'even', 'such', 'because',
	'turn', 'here', 'why', 'ask', 'went', 'men', 'read', 'need', 'land', 'different', 'home',
	'us', 'move', 'try', 'kind', 'hand', 'picture', 'again', 'change', 'off', 'play', 'spell',
	'air', 'away', 'animal', 'house', 'point', 'page', 'letter', 'mother', 'answer', 'found',
	'study', 'still', 'learn', 'should', 'world', 'high', 'every', 'near', 'add', 'food',
	'between', 'own', 'below', 'country', 'plant', 'last', 'school', 'father', 'keep', 'tree',
	'never', 'start', 'city', 'earth', 'eye', 'light', 'thought', 'head', 'under', 'story',
	'saw', 'left', 'few', 'while', 'along', 'might', 'close', 'something', 'seem', 'next',
	'hard', 'open', 'example', 'begin', 'life', 'always', 'those', 'both', 'paper', 'together',
	'got', 'group', 'often', 'run', 'important', 'until', 'children', 'side', 'feet', 'car',
	'mile', 'night', 'walk', 'white', 'sea', 'began', 'grow', 'took', 'river', 'four', 'carry',
	'state', 'once', 'book', 'hear', 'stop', 'without', 'second', 'later', 'miss', 'idea',
	'enough', 'eat', 'face', 'watch', 'far', 'really', 'almost', 'let', 'above', 'girl',
	'sometimes', 'mountain', 'cut', 'young', 'talk', 'soon', 'list', 'song', 'being', 'leave',
	'family', 'hello', 'hi', 'hey', 'help', 'hero', 'okay', 'yes', 'no', 'please', 'thanks',
]);

/** Minimal shape of the bits of the NSpell instance this file actually uses - avoids pulling in
 *  nspell's own (nonexistent) type declarations for two methods. */
interface NSpellInstance {
	correct(word: string): boolean;
	suggest(word: string): string[];
}

let spell: NSpellInstance | null = null;
let loadPromise: Promise<NSpellInstance | null> | null = null;

/** Fetches the bundled Hunspell en_US dictionary (en.aff + en.dic) and builds the nspell
 *  instance once; safe to call repeatedly or concurrently - every caller after the first awaits
 *  the same in-flight load. Resolves to null (rather than throwing) if the fetch fails, so a
 *  missing/blocked asset just quietly leaves spell-check disabled instead of breaking typing. */
export async function loadDictionary(): Promise<NSpellInstance | null> {
	if (spell) return spell;
	if (!loadPromise) {
		loadPromise = (async () => {
			try {
				const [affRes, dicRes] = await Promise.all([
					fetch('/dictionaries/en.aff'),
					fetch('/dictionaries/en.dic'),
				]);
				if (!affRes.ok) throw new Error(`Dictionary affix fetch failed: ${affRes.status}`);
				if (!dicRes.ok) throw new Error(`Dictionary word-list fetch failed: ${dicRes.status}`);
				const [aff, dic] = await Promise.all([affRes.text(), dicRes.text()]);
				spell = nspell({ aff, dic }) as NSpellInstance;
				return spell;
			} catch {
				loadPromise = null;
				return null;
			}
		})();
	}
	return loadPromise;
}

/** Whether the dictionary has finished loading and suggestCorrection()/isKnownWord() are ready
 *  to use synchronously. Callers kick off loadDictionary() once (e.g. when spell-check is
 *  turned on or a note is opened) and just skip checking words until this is true. */
export function isDictionaryReady(): boolean {
	return spell !== null;
}

/** Whether `word` (as typed, any case) is a recognized word - a thin, cheap wrapper around
 *  nspell's own correct(), which already handles casing (HELLO/Hello/hello all match "hello"),
 *  contractions ("don't"/"isn't" are dictionary entries in their own right), and the curly
 *  right single quote TipTap's Typography extension substitutes for a typed "'" mid-word (the
 *  bundled dictionary's ICONV table normalizes U+2019 to a plain apostrophe before lookup, so
 *  no manual normalization is needed here). Returns true (don't flag anything) if the
 *  dictionary hasn't loaded yet. This is the check scanDocumentForMisspellings() in
 *  Editor.svelte should use for "is this word wrong" - it's a single dictionary/trie lookup,
 *  unlike suggestCorrection()'s suggest() call below, which is meaningfully more expensive and
 *  should only run for the one word actually being corrected. */
export function isKnownWord(word: string): boolean {
	if (!spell) return true;
	return spell.correct(word);
}

// How two words compare, used only to rank nspell's own suggestion list (rerankSuggestions()
// below) - 0 transposition, 1 substitution, 2 deletion, 3 insertion, 4 "other" (not a single
// clean edit between the two, e.g. because nspell reached it via an affix/compound rule rather
// than a plain character edit). Roughly in order of how common each typo pattern actually is.
type EditKind = 0 | 1 | 2 | 3 | 4;

/** Classifies the edit that turns `a` into `b` (both already lowercased), when it's a single
 *  clean transposition/substitution/deletion/insertion - cheap to run over nspell's own
 *  (already short, already-real-word) suggestion list, unlike generating every possible edit of
 *  a word from scratch. */
function editKindBetween(a: string, b: string): EditKind {
	if (a === b) return 4;
	if (a.length === b.length) {
		const diff: number[] = [];
		for (let i = 0; i < a.length; i++) {
			if (a[i] !== b[i]) diff.push(i);
			if (diff.length > 2) break;
		}
		if (diff.length === 1) return 1; // substitution
		if (
			diff.length === 2 &&
			diff[1] === diff[0] + 1 &&
			a[diff[0]] === b[diff[1]] &&
			a[diff[1]] === b[diff[0]]
		) {
			return 0; // adjacent transposition
		}
		return 4;
	}
	if (a.length === b.length + 1) {
		for (let i = 0; i < a.length; i++) {
			if (a.slice(0, i) + a.slice(i + 1) === b) return 2; // deletion (a -> b)
		}
		return 4;
	}
	if (a.length === b.length - 1) {
		for (let i = 0; i < b.length; i++) {
			if (b.slice(0, i) + b.slice(i + 1) === a) return 3; // insertion (a -> b)
		}
		return 4;
	}
	return 4;
}

/** Re-sorts nspell's own suggestion list: a word in COMMON_WORDS first, full stop, before
 *  anything else is considered; among ties, the edit pattern more likely to be a real typo
 *  (see EditKind above); and as a final tiebreak, nspell's own original relative order, which
 *  still encodes useful signal (keyboard adjacency, phonetic similarity) this function doesn't
 *  otherwise account for. Operates purely on the strings nspell already returned - never adds a
 *  candidate nspell didn't surface itself. */
function rerankSuggestions(original: string, suggestions: string[]): string[] {
	const lowerOriginal = original.toLowerCase();
	return suggestions
		.map((word, index) => ({
			word,
			index,
			common: COMMON_WORDS.has(word.toLowerCase()),
			kind: editKindBetween(lowerOriginal, word.toLowerCase()),
		}))
		.sort((a, b) => {
			if (a.common !== b.common) return a.common ? -1 : 1;
			if (a.kind !== b.kind) return a.kind - b.kind;
			return a.index - b.index;
		})
		.map((c) => c.word);
}

const ALL_UPPER_RE = /^[A-Z]+$/;

// nspell's suggest() is a meaningfully more expensive call than correct() (it's doing real
// suggestion generation, not a single dictionary lookup - see the timing note in
// getRankedSuggestions() below), and the same misspelled word tends to recur a lot within one
// note (a name, a typo the person keeps making). Cache the reranked list per word rather than
// recomputing it on every keystroke near that word; capped and cleared-on-overflow rather than
// LRU-evicted, since a genuine cap hit is rare enough that O(1) beats the bookkeeping.
const RANKED_CACHE_MAX = 5000;
const rankedCache = new Map<string, string[]>();

function getRankedSuggestions(word: string): string[] {
	const cached = rankedCache.get(word);
	if (cached !== undefined) return cached;
	// suggest() on this dictionary runs roughly ~1ms/call in practice - fine for the one active
	// word under the cursor or a right-click menu, but never call this from a whole-document
	// scan (use isKnownWord()/correct() there instead, which is orders of magnitude cheaper).
	const ranked = rerankSuggestions(word, spell!.suggest(word));
	if (rankedCache.size >= RANKED_CACHE_MAX) rankedCache.clear();
	rankedCache.set(word, ranked);
	return ranked;
}

/** Public entry point: given a word as typed (any case), returns a suggested correction, or
 *  null if the word looks fine, is too short/unusual to bother checking, or the dictionary
 *  hasn't loaded yet (same as "nothing wrong found"). Capitalization of the result already
 *  matches the input - nspell's suggest() does this itself. */
export function suggestCorrection(word: string): string | null {
	if (!spell) return null;
	// Skip words that can't usefully be spell-checked: too short to bother the user over, or
	// ALL CAPS (almost always an acronym, not a typo - though the bundled dictionary already
	// recognizes plenty of real ones, like "NASA", via correct() below on its own).
	if (word.length < 3 || ALL_UPPER_RE.test(word)) return null;
	if (spell.correct(word)) return null;
	return getRankedSuggestions(word)[0] ?? null;
}

/** Up to `limit` (default 3) plausible corrections for a misspelled word, best guess first -
 *  for a "pick one" UI (the right-click suggestion menu) rather than suggestCorrection()'s
 *  single guess. Returns [] for a word that isn't misspelled in the first place (same rules as
 *  suggestCorrection()), same as returning null there - the caller shouldn't be showing this
 *  menu at all in that case. */
export function suggestCorrections(word: string, limit = 3): string[] {
	if (!spell) return [];
	if (word.length < 3 || ALL_UPPER_RE.test(word)) return [];
	if (spell.correct(word)) return [];
	return getRankedSuggestions(word).slice(0, limit);
}
