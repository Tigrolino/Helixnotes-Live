import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';

// spellcheck.ts fetches its wordlist from the app's static assets at runtime
// (fetch('/dictionaries/en.txt')) - stand that in with the real bundled file read straight off
// disk, so these tests exercise the actual shipped dictionary rather than a fake stand-in.
const dictPath = new URL('../../../static/dictionaries/en.txt', import.meta.url);
const dictText = fs.readFileSync(dictPath, 'utf8');
globalThis.fetch = async (url) => {
	if (String(url).includes('dictionaries/en.txt')) {
		return { ok: true, text: async () => dictText };
	}
	throw new Error(`unexpected fetch in test: ${url}`);
};

const { loadDictionary, isDictionaryReady, isKnownWord, suggestCorrection } = await import(
	new URL('./spellcheck.ts', import.meta.url)
);

// node:test only actually runs registered tests once the whole module finishes evaluating
// (including any top-level await below) - so calling isDictionaryReady()/suggestCorrection()
// *inside* a test() callback would always see the post-load state, regardless of where that
// test() call sits relative to loadDictionary(). Capture the pre-load state as plain values
// right here, synchronously, before anything is awaited.
const readyBeforeLoad = isDictionaryReady();
const suggestionBeforeLoad = suggestCorrection('helllo');

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
	assert.equal(suggestCorrection('wrold'), 'world'); // transposition
	assert.equal(suggestCorrection('wolrd'), 'world'); // transposition
});

test('capitalization of the original word is preserved in the suggestion', () => {
	assert.equal(suggestCorrection('Teh'), 'The');
});

test('contractions are recognized and not flagged, even though the wordlist has no apostrophes', () => {
	for (const w of ["don't", "I'm", "they're", "it's", "we've", "isn't", "wouldn't"]) {
		assert.equal(isKnownWord(w), true, `expected "${w}" to be known`);
		assert.equal(suggestCorrection(w), null, `expected no suggestion for "${w}"`);
	}
});

test('curly right single quotes (Typography-converted apostrophes) are normalized', () => {
	// TipTap's Typography extension turns a typed "'" mid-word into U+2019 - "don't" is
	// actually stored as "don\u2019t".
	for (const w of ['don\u2019t', 'they\u2019re', 'it\u2019s', 'I\u2019m']) {
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
