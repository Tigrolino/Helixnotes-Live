import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';

// spellcheck.ts fetches the bundled Hunspell dictionary from the app's static assets at runtime
// (fetch('/dictionaries/en.aff') + fetch('/dictionaries/en.dic')) - stand that in with the real
// bundled files read straight off disk, so these tests exercise the actual shipped dictionary
// rather than a fake stand-in.
const affPath = new URL('../../../static/dictionaries/en.aff', import.meta.url);
const dicPath = new URL('../../../static/dictionaries/en.dic', import.meta.url);
const affText = fs.readFileSync(affPath, 'utf8');
const dicText = fs.readFileSync(dicPath, 'utf8');
globalThis.fetch = async (url) => {
	const s = String(url);
	if (s.includes('dictionaries/en.aff')) return { ok: true, text: async () => affText };
	if (s.includes('dictionaries/en.dic')) return { ok: true, text: async () => dicText };
	throw new Error(`unexpected fetch in test: ${url}`);
};

const { loadDictionary, isDictionaryReady, isKnownWord, suggestCorrection, suggestCorrections } = await import(
	new URL('./spellcheck.ts', import.meta.url)
);

// node:test only actually runs registered tests once the whole module finishes evaluating
// (including any top-level await below) - so calling isDictionaryReady()/suggestCorrection()
// *inside* a test() callback would always see the post-load state, regardless of where that
// test() call sits relative to loadDictionary(). Capture the pre-load state as plain values
// right here, synchronously, before anything is awaited.
const readyBeforeLoad = isDictionaryReady();
const suggestionBeforeLoad = suggestCorrection('helllo');
const suggestionsBeforeLoad = suggestCorrections('helllo');

test('suggestCorrection returns null before the dictionary has loaded', () => {
	assert.equal(readyBeforeLoad, false);
	assert.equal(suggestionBeforeLoad, null);
});

await loadDictionary();

test('dictionary is ready after loadDictionary() resolves', () => {
	assert.equal(isDictionaryReady(), true);
});

test('loadDictionary() is idempotent - concurrent/repeat calls share one load', async () => {
	const [a, b] = await Promise.all([loadDictionary(), loadDictionary()]);
	assert.equal(a, b);
});

test('recognizes ordinary correctly-spelled words', () => {
	for (const w of ['the', 'store', 'walked', 'coffee', 'beautiful', 'notebook']) {
		assert.equal(isKnownWord(w), true, `expected "${w}" to be known`);
	}
});

test('recognizes known words case-insensitively', () => {
	assert.equal(isKnownWord('Hello'), true);
	assert.equal(isKnownWord('WORLD'), true);
});

test('does not flag correctly-spelled words', () => {
	for (const w of ['the', 'store', 'walked', 'coffee', 'beautiful', 'notebook']) {
		assert.equal(suggestCorrection(w), null, `expected no suggestion for "${w}"`);
	}
});

test('common single-typo transpositions are corrected', () => {
	assert.equal(suggestCorrection('teh'), 'the');
	assert.equal(suggestCorrection('adn'), 'and');
	assert.equal(suggestCorrection('recieve'), 'receive');
});

test('missing-letter and extra-letter typos are corrected', () => {
	assert.equal(suggestCorrection('wrold'), 'world');
	assert.equal(suggestCorrection('wolrd'), 'world');
});

test('capitalization of the original word is preserved in the suggestion', () => {
	assert.equal(suggestCorrection('Teh'), 'The');
});

test('contractions are recognized and not flagged - the bundled Hunspell dictionary carries them as entries in their own right', () => {
	for (const w of ["don't", "I'm", "they're", "it's", "we've", "isn't", "wouldn't"]) {
		assert.equal(isKnownWord(w), true, `expected "${w}" to be known`);
		assert.equal(suggestCorrection(w), null, `expected no suggestion for "${w}"`);
	}
});

test('curly right single quotes (Typography-converted apostrophes) are normalized', () => {
	// TipTap's Typography extension turns a typed "'" mid-word into U+2019 - "don't" is
	// actually stored as "don’t". The dictionary's own ICONV table normalizes this before
	// lookup, so no manual normalization is needed in spellcheck.ts itself.
	for (const w of ['don’t', 'they’re', 'it’s', 'I’m']) {
		assert.equal(isKnownWord(w), true, `expected "${w}" to be known`);
		assert.equal(suggestCorrection(w), null, `expected no suggestion for "${w}"`);
	}
});

test('short words are left alone even if not in the dictionary', () => {
	assert.equal(suggestCorrection('xz'), null);
});

test('ALL-CAPS words are treated as acronyms, not typos', () => {
	assert.equal(suggestCorrection('TEH'), null);
	assert.equal(suggestCorrection('NASA'), null);
});

test('a real (if uncommon) word is never "corrected" into a different real word', () => {
	// Sanity check against over-eager suggestions: a dictionary word should never come back
	// from suggestCorrection at all, common or not.
	for (const w of ['xylophone', 'quartz', 'rhythm']) {
		assert.equal(suggestCorrection(w), null, `expected no suggestion for real word "${w}"`);
	}
});

test('gibberish far from any real word yields no suggestion rather than a wild guess', () => {
	assert.equal(suggestCorrection('zxqvwrbpl'), null);
});

test('suggestCorrections returns several ranked candidates for an ambiguous typo', () => {
	const options = suggestCorrections('helo');
	assert.ok(Array.isArray(options));
	assert.ok(options.length >= 1 && options.length <= 3);
	// "help" (substitution) and "hello" (insertion) are both one edit away and both common
	// words - substitution ranks first (see EditKind ordering in rerankSuggestions()).
	assert.equal(options[0], 'help');
	assert.ok(options.includes('hello'), `expected 'hello' among ${JSON.stringify(options)}`);
	// No duplicates.
	assert.equal(new Set(options).size, options.length);
});

test('suggestCorrections respects a custom limit', () => {
	const options = suggestCorrections('helo', 1);
	assert.equal(options.length, 1);
	assert.equal(options[0], 'help');
});

test('suggestCorrections returns [] for a correctly-spelled word', () => {
	assert.deepEqual(suggestCorrections('hello'), []);
});

test('suggestCorrections returns [] before the dictionary has loaded, same as suggestCorrection', () => {
	assert.deepEqual(suggestionsBeforeLoad, []);
});

test('suggestCorrections preserves capitalization across every returned candidate', () => {
	const options = suggestCorrections('Teh');
	assert.ok(options.length > 0);
	for (const w of options) {
		assert.equal(w[0], w[0].toUpperCase(), `expected ${JSON.stringify(w)} capitalized`);
	}
	assert.equal(options[0], 'The');
});

test('suggestCorrections returns [] for gibberish with nothing close, same as suggestCorrection', () => {
	assert.deepEqual(suggestCorrections('zxqvwrbpl'), []);
});

test('a common word beats an obscure one nspell would otherwise rank first (the original "teh"/"eth" tie-break bug)', () => {
	// Raw Hunspell suggest('adn') ranks "an" ahead of "and" purely on its own internal
	// heuristics - both are real one-edit-away words, but "and" is what a person overwhelmingly
	// means by "adn". This is what rerankSuggestions()'s common-word-first pass exists to fix.
	assert.equal(suggestCorrection('adn'), 'and');
});
