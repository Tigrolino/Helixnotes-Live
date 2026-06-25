<script lang="ts">
	import { onMount } from 'svelte';
	import { tags } from '$lib/stores/app';

	let { existing = [], placeholder = 'Add tag...', onsubmit, oncancel }: {
		existing?: string[];
		placeholder?: string;
		onsubmit: (tag: string) => void;
		oncancel: () => void;
	} = $props();

	let query = $state('');
	let selIndex = $state(-1);
	let inputEl = $state<HTMLInputElement>(null!);

	// Existing tags that match what's typed, prefix matches first, already-applied ones excluded.
	const suggestions = $derived.by(() => {
		const q = query.trim().toLowerCase();
		const taken = new Set(existing);
		const names = $tags
			.map(([name]) => name)
			.filter((name) => !taken.has(name) && (!q || name.toLowerCase().includes(q)));
		names.sort((a, b) => {
			const ap = a.toLowerCase().startsWith(q) ? 0 : 1;
			const bp = b.toLowerCase().startsWith(q) ? 0 : 1;
			if (ap !== bp) return ap - bp;
			return a.localeCompare(b);
		});
		return names.slice(0, 8);
	});

	function submit(tag: string) {
		if (!tag.trim()) return;
		onsubmit(tag);
		query = '';
		selIndex = -1;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			if (suggestions.length) { e.preventDefault(); selIndex = (selIndex + 1) % suggestions.length; }
		} else if (e.key === 'ArrowUp') {
			if (suggestions.length) { e.preventDefault(); selIndex = (selIndex - 1 + suggestions.length) % suggestions.length; }
		} else if (e.key === 'Enter') {
			e.preventDefault();
			submit(selIndex >= 0 && selIndex < suggestions.length ? suggestions[selIndex] : query);
		} else if (e.key === 'Tab') {
			// Complete the input to the highlighted suggestion without committing.
			if (selIndex >= 0 && selIndex < suggestions.length) { e.preventDefault(); query = suggestions[selIndex]; selIndex = -1; }
		} else if (e.key === 'Escape') {
			e.preventDefault();
			if (selIndex >= 0) selIndex = -1; // first Escape just drops the highlight
			else oncancel();
		}
	}

	onMount(() => inputEl?.focus());
</script>

<div class="tag-suggest">
	<input
		bind:this={inputEl}
		type="text"
		class="tag-suggest-input"
		{placeholder}
		bind:value={query}
		oninput={() => (selIndex = -1)}
		onkeydown={onKeydown}
	/>
	{#if suggestions.length}
		<div class="tag-suggest-list">
			{#each suggestions as s, i}
				<button
					type="button"
					class="tag-suggest-item"
					class:selected={i === selIndex}
					onmouseenter={() => (selIndex = i)}
					onmousedown={(e) => { e.preventDefault(); submit(s); }}
				>#{s}</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.tag-suggest {
		display: flex;
		flex-direction: column;
	}
	.tag-suggest-input {
		width: 100%;
		padding: 4px 8px;
		border: 1px solid var(--border-color);
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 12px;
		outline: none;
		font-family: inherit;
	}
	.tag-suggest-input:focus {
		border-color: var(--accent);
	}
	.tag-suggest-list {
		margin-top: 4px;
		max-height: 180px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.tag-suggest-item {
		text-align: left;
		padding: 5px 8px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--text-secondary);
		font-size: 12px;
		cursor: pointer;
	}
	.tag-suggest-item:hover,
	.tag-suggest-item.selected {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
</style>
