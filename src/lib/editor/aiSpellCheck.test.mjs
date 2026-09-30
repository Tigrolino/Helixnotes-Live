import assert from 'node:assert/strict';
import test from 'node:test';

const { buildSpellCheckPrompt, parseSpellCheckResponse } = await import(
	new URL('./aiSpellCheck.ts', import.meta.url)
);

test('buildSpellCheckPrompt numbers each block as "<index>: <text>"', () => {
	const prompt = buildSpellCheckPrompt([
		{ index: 0, text: 'Teh quick brown fox.' },
		{ index: 1, text: 'Another paragraph here.' },
	]);
	assert.equal(prompt, '0: Teh quick brown fox.\n1: Another paragraph here.');
});

test('buildSpellCheckPrompt handles an empty block list', () => {
	assert.equal(buildSpellCheckPrompt([]), '');
});

test('parseSpellCheckResponse parses a well-formed JSON array', () => {
	const entries = parseSpellCheckResponse('[{"block": 0, "word": "teh", "suggestions": ["the", "ten"]}]');
	assert.deepEqual(entries, [{ block: 0, word: 'teh', suggestions: ['the', 'ten'] }]);
});

test('parseSpellCheckResponse returns [] for an empty array response', () => {
	assert.deepEqual(parseSpellCheckResponse('[]'), []);
});

test('parseSpellCheckResponse strips a markdown code fence the model added despite instructions', () => {
	const raw = '```json\n[{"block": 1, "word": "adn", "suggestions": ["and"]}]\n```';
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 1, word: 'adn', suggestions: ['and'] }]);
});

test('parseSpellCheckResponse strips a fence with no "json" language tag', () => {
	const raw = '```\n[{"block": 0, "word": "wolrd", "suggestions": ["world"]}]\n```';
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 0, word: 'wolrd', suggestions: ['world'] }]);
});

test('parseSpellCheckResponse returns [] for unparseable JSON rather than throwing', () => {
	assert.deepEqual(parseSpellCheckResponse('not json at all'), []);
	assert.deepEqual(parseSpellCheckResponse(''), []);
	assert.deepEqual(parseSpellCheckResponse('{"not": "an array"}'), []);
});

test('parseSpellCheckResponse drops entries missing a required field', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'teh' }, // missing suggestions
		{ word: 'adn', suggestions: ['and'] }, // missing block
		{ block: 1, suggestions: ['the'] }, // missing word
		{ block: 2, word: 'recieve', suggestions: ['receive'] }, // valid
	]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 2, word: 'recieve', suggestions: ['receive'] }]);
});

test('parseSpellCheckResponse caps suggestions at 3, keeping order', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'helo', suggestions: ['hello', 'help', 'halo', 'held', 'helot'] },
	]);
	assert.deepEqual(parseSpellCheckResponse(raw), [
		{ block: 0, word: 'helo', suggestions: ['hello', 'help', 'halo'] },
	]);
});

test('parseSpellCheckResponse drops a suggestion equal to the word itself (case-insensitively), keeping the rest', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'World', suggestions: ['world', 'word', 'worlds'] },
	]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 0, word: 'World', suggestions: ['word', 'worlds'] }]);
});

test('parseSpellCheckResponse drops an entry whose suggestions are all equal to the word itself', () => {
	const raw = JSON.stringify([{ block: 0, word: 'hello', suggestions: ['hello', 'Hello'] }]);
	assert.deepEqual(parseSpellCheckResponse(raw), []);
});

test('parseSpellCheckResponse drops an entry whose suggestions field is missing or not an array', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'teh' },
		{ block: 1, word: 'adn', suggestions: 'and' },
		{ block: 2, word: 'wolrd', suggestions: ['world'] },
	]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 2, word: 'wolrd', suggestions: ['world'] }]);
});

test('parseSpellCheckResponse de-duplicates suggestions within one entry', () => {
	const raw = JSON.stringify([{ block: 0, word: 'teh', suggestions: ['the', 'The', 'the', 'ten'] }]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 0, word: 'teh', suggestions: ['the', 'ten'] }]);
});

test('parseSpellCheckResponse de-duplicates by (block, lowercased word), keeping the last one', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'teh', suggestions: ['ten'] },
		{ block: 0, word: 'Teh', suggestions: ['the'] },
	]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 0, word: 'Teh', suggestions: ['the'] }]);
});

test('parseSpellCheckResponse keeps the same word flagged separately in different blocks', () => {
	const raw = JSON.stringify([
		{ block: 0, word: 'teh', suggestions: ['the'] },
		{ block: 3, word: 'teh', suggestions: ['the'] },
	]);
	const entries = parseSpellCheckResponse(raw);
	assert.equal(entries.length, 2);
});

test('parseSpellCheckResponse ignores non-object array entries instead of throwing', () => {
	const raw = JSON.stringify(['not an object', 42, null, { block: 0, word: 'adn', suggestions: ['and'] }]);
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 0, word: 'adn', suggestions: ['and'] }]);
});

// The following cases all come from real, verbatim raw responses logged during live testing
// against an actual (occasionally unreliable) configured model - each one previously lost an
// entire chunk's worth of correctly-found results to a JSON parse failure over a few trailing
// characters, well after the last complete, well-formed entry.

test('parseSpellCheckResponse recovers entries when the array never got its closing bracket', () => {
	// Real observed shape: a dangling `"]}` tacked on after the last complete entry, with no `]`
	// ever closing the array itself.
	const raw = '```json\n[{"block": 0, "word": "realy", "suggestions": ["really"]}, {"block": 16, "word": "makes", "suggestions": ["make"]}"]}\n```';
	assert.deepEqual(parseSpellCheckResponse(raw), [
		{ block: 0, word: 'realy', suggestions: ['really'] },
		{ block: 16, word: 'makes', suggestions: ['make'] },
	]);
});

test('parseSpellCheckResponse recovers entries when there is a stray extra closing bracket', () => {
	// Real observed shape: `...["don't"]}]]` - one `]` too many after an otherwise well-formed,
	// properly closed array.
	const raw = '[{"block": 4, "word": "dont", "suggestions": ["don\'t"]}, {"block": 12, "word": "dont", "suggestions": ["do not"]}]]';
	assert.deepEqual(parseSpellCheckResponse(raw), [
		{ block: 4, word: 'dont', suggestions: ["don't"] },
		{ block: 12, word: 'dont', suggestions: ['do not'] },
	]);
});

test('parseSpellCheckResponse recovers entries when a markdown fence opens but never closes', () => {
	const raw = '```json\n[{"block": 2, "word": "Ment", "suggestions": ["mean"]}]';
	assert.deepEqual(parseSpellCheckResponse(raw), [{ block: 2, word: 'Ment', suggestions: ['mean'] }]);
});

test('parseSpellCheckResponse gives up (returns []) rather than guessing at a corrupted entry in the middle of the array', () => {
	// Real observed shape: a suggestion string opened with " and closed with ' instead, which
	// corrupts everything from that point on - this must NOT be "recovered" into something that
	// looks plausible, since there's no safe way to know what the model actually meant.
	const raw = '[{"block": 0, "word": "aplication", "suggestions": ["application\']}, {"block": 2, "word": "uses", "suggestions": ["use"]}]';
	assert.deepEqual(parseSpellCheckResponse(raw), []);
});
