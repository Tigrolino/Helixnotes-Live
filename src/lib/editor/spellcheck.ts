// Lightweight, dependency-free offline spell checker backing the "Spelling corrections"
// feature: a flat English wordlist (static/dictionaries/en.txt, see that folder's README
// for provenance/license) plus a small Norvig-style edit-distance suggester. No AI call, no
// network access after the one-time local asset fetch, and works with no AI provider
// configured at all - unlike ghost-text, this has nothing to do with $appConfig.ai_provider.

const LETTERS = 'abcdefghijklmnopqrstuvwxyz';

// Trailing contraction pieces ("don't" -> "do" + "n't") aren't worth listing as separate
// dictionary entries - stripped and the stem re-checked before a contraction gets flagged
// just because the wordlist only has its bare stem.
const CONTRACTION_SUFFIXES = ["'s", "'t", "'re", "'ll", "'ve", "'d", "'m"];

// A couple hundred of the most frequent words in English (articles, pronouns, prepositions,
// conjunctions, and other everyday function/content words) - used only to break ties when
// more than one dictionary word is an equally-plausible correction (see pickBest() below).
// The 370k-word dictionary itself has no frequency data, so without this, a tie between a
// very common word and an obscure one (e.g. "teh" is one transposition away from both "the"
// and "eth", the letter <eth>) would be broken alphabetically and could easily pick the
// obscure one.
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
	'family', 'hello', 'hi', 'hey', 'help', 'hero', 'okay', 'yes', 'no', 'please', 'thanks'
]);

let dictionary: Set<string> | null = null;
let loadPromise: Promise<Set<string> | null> | null = null;

/** Fetches and parses the bundled wordlist once; safe to call repeatedly or concurrently -
 *  every caller after the first awaits the same in-flight load. Resolves to null (rather
 *  than throwing) if the fetch fails, so a missing/blocked asset just quietly leaves spell-
 *  check disabled instead of breaking typing. */
export async function loadDictionary(): Promise<Set<string> | null> {
	if (dictionary) return dictionary;
	if (!loadPromise) {
		loadPromise = (async () => {
			try {
				const res = await fetch('/dictionaries/en.txt');
				if (!res.ok) throw new Error(`Dictionary fetch failed: ${res.status}`);
				const text = await res.text();
				const set = new Set<string>();
				for (const line of text.split('\n')) {
					const w = line.trim();
					if (w) set.add(w);
				}
				dictionary = set;
				return set;
			} catch {
				loadPromise = null;
				return null;
			}
		})();
	}
	return loadPromise;
}

/** Whether the dictionary has finished loading and suggestCorrection()/isKnownWord() are
 *  ready to use synchronously. Callers kick off loadDictionary() once (e.g. when spell-check
 *  is turned on or a note is opened) and just skip checking words until this is true. */
export function isDictionaryReady(): boolean {
	return dictionary !== null;
}

function stripContraction(word: string): string {
	for (const suf of CONTRACTION_SUFFIXES) {
		if (word.length > suf.length && word.endsWith(suf)) return word.slice(0, -suf.length);
	}
	return word;
}

/** Whether `word` (as typed, any case) is a recognized word: checked lowercased, and if not
 *  found there, with a trailing contraction piece ("n't", "'re"...) stripped, so "don't" or
 *  "they're" aren't flagged just because the dictionary only has the bare stem. Returns true
 *  (don't flag anything) if the dictionary hasn't loaded yet. */
export function isKnownWord(word: string): boolean {
	if (!dictionary) return true;
	// TipTap's Typography extension (already on in this app) auto-converts a straight "'"
	// typed mid-word into a curly right single quote (U+2019) - "don't" ends up stored in the
	// document as "don\u2019t". Normalize back to a plain apostrophe before lookup so that
	// doesn't get flagged just because the dictionary and CONTRACTION_SUFFIXES only know the
	// straight one.
	const lower = word.toLowerCase().replace(/\u2019/g, "'");
	if (dictionary.has(lower)) return true;
	const stem = stripContraction(lower);
	return stem !== lower && dictionary.has(stem);
}

// 0 transposition, 1 substitution, 2 deletion, 3 insertion - roughly in order of how common
// each typo pattern actually is, used to rank candidate corrections when more than one
// edit-distance-1 word matches the dictionary.
type EditKind = 0 | 1 | 2 | 3;

/** Every string one single-letter edit away from `word` (one deletion, adjacent-pair
 *  transposition, substitution, or insertion), tagged with the best (lowest) EditKind it was
 *  reached by. Roughly 54*n+25 candidates for a word of length n - cheap enough to generate
 *  and hash-check against the dictionary on every completed word, and (for the edit-distance-2
 *  fallback below) cheap enough to do again for each of those. */
function edits1(word: string): Map<string, EditKind> {
	const out = new Map<string, EditKind>();
	const consider = (candidate: string, kind: EditKind) => {
		const existing = out.get(candidate);
		if (existing === undefined || kind < existing) out.set(candidate, kind);
	};
	for (let i = 0; i <= word.length; i++) {
		const left = word.slice(0, i);
		const right = word.slice(i);
		if (right.length >= 1) consider(left + right.slice(1), 2); // deletion
		if (right.length >= 2) consider(left + right[1] + right[0] + right.slice(2), 0); // transposition
		if (right.length >= 1) {
			for (const c of LETTERS) {
				if (c !== right[0]) consider(left + c + right.slice(1), 1); // substitution
			}
		}
		for (const c of LETTERS) consider(left + c + right, 3); // insertion
	}
	return out;
}

/** Picks one word out of a tied set of equally-plausible candidates: prefers a common word
 *  (see COMMON_WORDS) if any of the candidates are one, since a collision between a common
 *  word and an obscure one is exactly the case where "just pick alphabetically" tends to
 *  pick the wrong one. Falls back to alphabetical among whatever's left for determinism. */
function pickBest(words: string[]): string {
	const common = words.filter((w) => COMMON_WORDS.has(w));
	const pool = common.length ? common : words;
	return pool.sort()[0];
}

/** Best-effort single correction for a misspelled `lowerWord`, or null if nothing close
 *  enough was found. Tries every edit-distance-1 variant first, preferring the lowest
 *  EditKind found and breaking ties with pickBest(). Only if none of those are real words
 *  does it fall back to edit-distance-2, which is inherently fuzzier: more than a handful of
 *  equally-plausible matches there means "not confident enough" rather than guessing one. */
function bestCorrection(lowerWord: string, dict: Set<string>): string | null {
	const e1 = edits1(lowerWord);
	let bestKind: EditKind | null = null;
	let bestWords: string[] = [];
	for (const [candidate, kind] of e1) {
		if (!dict.has(candidate)) continue;
		if (bestKind === null || kind < bestKind) {
			bestKind = kind;
			bestWords = [candidate];
		} else if (kind === bestKind) {
			bestWords.push(candidate);
		}
	}
	if (bestWords.length) return pickBest(bestWords);

	const distance2 = new Set<string>();
	outer: for (const w1 of e1.keys()) {
		for (const w2 of edits1(w1).keys()) {
			if (dict.has(w2)) {
				distance2.add(w2);
				if (distance2.size > 4) break outer; // already too many candidates to be confident
			}
		}
	}
	if (distance2.size === 0 || distance2.size > 4) return null;
	return pickBest([...distance2]);
}

const ALL_UPPER_RE = /^[A-Z]+$/;

/** Public entry point: given a word as typed (any case), returns a suggested correction
 *  with capitalization matched to the original, or null if the word looks fine, is too
 *  short/unusual to bother checking, or no confident correction was found. Synchronous -
 *  call loadDictionary() ahead of time and check isDictionaryReady() before relying on this
 *  (it always returns null until the dictionary is loaded, same as "nothing wrong found"). */
export function suggestCorrection(word: string): string | null {
	if (!dictionary) return null;
	// Skip words that can't usefully be spell-checked: too short to bother the user over, or
	// ALL CAPS (almost always an acronym, not a typo). Anything containing digits, hyphens,
	// or other punctuation never reaches here in the first place - the caller's word-boundary
	// regex only pulls out letter(+apostrophe) runs to begin with.
	if (word.length < 3 || ALL_UPPER_RE.test(word)) return null;
	if (isKnownWord(word)) return null;
	const lower = word.toLowerCase().replace(/\u2019/g, "'");
	const correction = bestCorrection(lower, dictionary);
	if (!correction || correction === lower) return null;
	// The wordlist is all-lowercase - re-apply the original word's capitalization so "Teh"
	// corrects to "The", not "the".
	if (word[0] !== word[0].toLowerCase()) {
		return correction[0].toUpperCase() + correction.slice(1);
	}
	return correction;
}

/** A candidate correction together with what it took to reach it from the misspelled word -
 *  used only to rank candidates (see rankedCorrections() below), never returned as-is. */
type Candidate = { word: string; kind: EditKind; distance: 1 | 2 };

/** Sorts candidates for display. Unlike suggestCorrection()'s single guess - which ranks
 *  purely by EditKind (transposition, then substitution, then deletion, then insertion; see
 *  edits1()) because it only ever returns one answer and a rare-but-"closer" edit pattern is
 *  a reasonable tiebreaker for a single silent guess - a menu of several options is read by a
 *  person, and a 370k-word dictionary pulled from public wordlists has plenty of obscure or
 *  archaic entries sitting at edit-distance 1 (an early version of this ranking offered
 *  "halo"/"held"/"hele"/"helm" for "helo" ahead of the obviously-intended "hello", purely
 *  because they're substitutions and "hello" is an insertion). So a word in COMMON_WORDS is
 *  ranked ahead of one that isn't, full stop, before edit distance or kind are even
 *  considered; those still break ties within each group. */
function rankCandidates(candidates: Candidate[]): string[] {
	return [...candidates]
		.sort((a, b) => {
			const aCommon = COMMON_WORDS.has(a.word);
			const bCommon = COMMON_WORDS.has(b.word);
			if (aCommon !== bCommon) return aCommon ? -1 : 1;
			if (a.distance !== b.distance) return a.distance - b.distance;
			if (a.kind !== b.kind) return a.kind - b.kind;
			return a.word < b.word ? -1 : a.word > b.word ? 1 : 0;
		})
		.map((c) => c.word);
}

/** Up to `limit` distinct dictionary words plausibly meant by `lowerWord`, best guesses
 *  first - see rankCandidates() for how "best" is decided. Edit-distance-2 candidates are
 *  only considered if distance-1 didn't fill `limit` on its own. Unlike bestCorrection()
 *  (used by suggestCorrection(), which stays conservative and returns nothing rather than
 *  guess wrong), this is meant to hand several candidates to a person to pick from, so a
 *  genuinely ambiguous typo isn't a reason to hold anything back - it's exactly the case
 *  multiple suggestions are for. */
function rankedCorrections(lowerWord: string, dict: Set<string>, limit: number): string[] {
	const e1 = edits1(lowerWord);
	const seen = new Set<string>();
	const candidates: Candidate[] = [];
	for (const [candidate, kind] of e1) {
		if (candidate === lowerWord || seen.has(candidate) || !dict.has(candidate)) continue;
		seen.add(candidate);
		candidates.push({ word: candidate, kind, distance: 1 });
	}
	if (candidates.length < limit) {
		// Bounds the search, not the result - rankCandidates + the slice below still pick the
		// best `limit` out of however many distance-2 candidates turned up.
		let distance2Count = 0;
		outer: for (const [w1, k1] of e1) {
			for (const [w2, k2] of edits1(w1)) {
				if (w2 === lowerWord || seen.has(w2) || !dict.has(w2)) continue;
				seen.add(w2);
				candidates.push({ word: w2, kind: (Math.max(k1, k2) as EditKind), distance: 2 });
				if (++distance2Count > 24) break outer;
			}
		}
	}
	return rankCandidates(candidates).slice(0, limit);
}

/** Up to `limit` (default 3) plausible corrections for a misspelled word, best guess first -
 *  for a "pick one" UI (the right-click suggestion menu) rather than suggestCorrection()'s
 *  single conservative guess. Returns [] for a word that isn't misspelled in the first place
 *  (same rules as suggestCorrection(): too short, ALL CAPS, or already a dictionary word),
 *  same as returning null there - the caller shouldn't be showing this menu at all in that
 *  case. Capitalization of each suggestion is matched to the original word. */
export function suggestCorrections(word: string, limit = 3): string[] {
	if (!dictionary) return [];
	if (word.length < 3 || ALL_UPPER_RE.test(word)) return [];
	if (isKnownWord(word)) return [];
	const lower = word.toLowerCase().replace(/’/g, "'");
	const ranked = rankedCorrections(lower, dictionary, limit);
	const capitalize = word[0] !== word[0].toLowerCase();
	return ranked.map((w) => (capitalize ? w[0].toUpperCase() + w.slice(1) : w));
}
