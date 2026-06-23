<script lang="ts">
	import { showSearch, activeNote, activeNotePath, editorDirty, appConfig, mobileView } from '$lib/stores/app';
	import { searchNotes, readNote } from '$lib/api';
	import { debounce } from '$lib/utils/debounce';
	import type { SearchResult } from '$lib/types';
	import { isMobile } from '$lib/platform';

	let query = $state('');
	let results = $state<SearchResult[]>([]);
	let selectedIndex = $state(0);
	let inputEl = $state<HTMLInputElement>(null!);
	let resultsEl = $state<HTMLDivElement>(null!);

	const doSearch = debounce(async (q: string) => {
		if (!q.trim()) {
			results = [];
			return;
		}
		try {
			results = await searchNotes(q, 20);
			selectedIndex = 0;
		} catch (e) {
			console.error('Search failed:', e);
			results = [];
		}
	}, 150);

	$effect(() => {
		if ($showSearch && inputEl) {
			query = '';
			results = [];
			selectedIndex = 0;
			setTimeout(() => inputEl?.focus(), 50);
		}
	});

	function handleInput() {
		doSearch(query);
	}

	function scrollToSelected() {
		if (!resultsEl) return;
		const item = resultsEl.children[selectedIndex] as HTMLElement;
		if (item) item.scrollIntoView({ block: 'nearest' });
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			$showSearch = false;
		} else if (e.key === 'ArrowDown') {
			e.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, results.length - 1);
			scrollToSelected();
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, 0);
			scrollToSelected();
		} else if (e.key === 'Enter' && results.length > 0) {
			openResult(results[selectedIndex]);
		}
	}

	function getNotebook(path: string): string {
		const vault = $appConfig?.active_vault;
		if (!vault) return '';
		let rel = path;
		if (rel.startsWith(vault + '/')) rel = rel.substring(vault.length + 1);
		const lastSlash = rel.lastIndexOf('/');
		if (lastSlash <= 0) return '';
		return rel.substring(0, lastSlash);
	}

	function highlightSnippet(snippet: string, q: string): string {
		if (!q.trim() || !snippet) return escapeHtml(snippet);
		const words = q.trim().split(/\s+/).filter(w => w.length > 1);
		if (words.length === 0) return escapeHtml(snippet);
		const escaped = escapeHtml(snippet);
		const pattern = words.map(w => w.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|');
		const re = new RegExp(`(${pattern})`, 'gi');
		return escaped.replace(re, '<mark>$1</mark>');
	}

	function escapeHtml(str: string): string {
		return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
	}

	async function openResult(result: SearchResult) {
		try {
			const content = await readNote(result.path);
			$activeNote = content;
			$activeNotePath = result.path;
			$editorDirty = false;
			$showSearch = false;
			if (isMobile) $mobileView = 'editor';
		} catch (e) {
			console.error('Failed to open search result:', e);
		}
	}

	function close() {
		$showSearch = false;
	}
</script>

{#if $showSearch}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="search-overlay" onclick={close} onkeydown={handleKeydown}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="search-panel" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
			<div class="search-input-wrapper">
				<svg class="search-icon" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<circle cx="11" cy="11" r="8" />
					<line x1="21" y1="21" x2="16.65" y2="16.65" />
				</svg>
				<input
					bind:this={inputEl}
					type="text"
					placeholder="Search notes..."
					bind:value={query}
					oninput={handleInput}
					onkeydown={handleKeydown}
				/>
				{#if !isMobile}<kbd class="search-esc">Esc</kbd>{/if}
			</div>

			{#if results.length > 0}
				<div class="search-results" bind:this={resultsEl}>
					{#each results as result, i}
						<button
							class="result-item"
							class:selected={i === selectedIndex}
							onclick={() => openResult(result)}
							onmouseenter={() => (selectedIndex = i)}
						>
							<div class="result-header">
								<svg class="result-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
									<path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
									<polyline points="14 2 14 8 20 8" />
									<line x1="16" y1="13" x2="8" y2="13" />
									<line x1="16" y1="17" x2="8" y2="17" />
								</svg>
								<span class="result-title">{result.title}</span>
								{#if getNotebook(result.path)}
									<span class="result-notebook">{getNotebook(result.path)}</span>
								{/if}
							</div>
							{#if result.snippet}
								<span class="result-snippet">{@html highlightSnippet(result.snippet, query)}</span>
							{/if}
						</button>
					{/each}
				</div>
				<div class="search-footer">
					<span class="search-count">{results.length} result{results.length !== 1 ? 's' : ''}</span>
					<span class="search-hints">
						<kbd>&uarr;</kbd><kbd>&darr;</kbd> navigate
						<kbd>&crarr;</kbd> open
					</span>
				</div>
			{:else if query.trim()}
				<div class="no-results">
					<svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" opacity="0.3">
						<circle cx="11" cy="11" r="8" />
						<line x1="21" y1="21" x2="16.65" y2="16.65" />
						<line x1="8" y1="8" x2="14" y2="14" />
						<line x1="14" y1="8" x2="8" y2="14" />
					</svg>
					<span>No results for "{query}"</span>
				</div>
			{:else}
				<div class="search-empty">
					<span>Type to search across all notes</span>
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.search-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		backdrop-filter: blur(4px);
		-webkit-backdrop-filter: blur(4px);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding-top: 15vh;
		z-index: 2000;
		animation: overlay-in 0.15s ease-out;
	}

	@keyframes overlay-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	@keyframes panel-in {
		from { opacity: 0; transform: translateY(-8px) scale(0.98); }
		to { opacity: 1; transform: translateY(0) scale(1); }
	}

	.search-panel {
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 14px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(255, 255, 255, 0.05) inset;
		width: 620px;
		max-height: 460px;
		overflow: hidden;
		display: flex;
		flex-direction: column;
		animation: panel-in 0.15s ease-out;
	}

	:global(:root.dark) .search-panel {
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255, 255, 255, 0.06) inset;
	}

	.search-input-wrapper {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 16px 18px;
		border-bottom: 1px solid var(--border-light);
	}

	.search-icon {
		color: var(--text-tertiary);
		flex-shrink: 0;
	}

	.search-input-wrapper input {
		flex: 1;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 17px;
		outline: none;
		font-weight: 400;
	}

	.search-input-wrapper input::placeholder {
		color: var(--text-tertiary);
	}

	.search-esc {
		font-size: 11px;
		color: var(--text-tertiary);
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 2px 6px;
		flex-shrink: 0;
		font-family: inherit;
		line-height: 1.2;
	}

	.search-results {
		overflow-y: auto;
		padding: 6px;
		flex: 1;
	}

	.result-item {
		display: block;
		width: 100%;
		padding: 10px 12px;
		border: none;
		background: none;
		border-radius: 8px;
		cursor: pointer;
		text-align: left;
		transition: background 0.1s;
	}

	.result-item.selected {
		background: var(--bg-hover);
	}

	.result-header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.result-icon {
		color: var(--text-tertiary);
		flex-shrink: 0;
		opacity: 0.6;
	}

	.result-title {
		font-weight: 500;
		font-size: 14px;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.result-notebook {
		margin-left: auto;
		font-size: 11px;
		color: var(--text-tertiary);
		background: var(--bg-tertiary);
		padding: 1px 7px;
		border-radius: 4px;
		flex-shrink: 0;
		max-width: 160px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.result-snippet {
		display: block;
		font-size: 12px;
		color: var(--text-tertiary);
		margin-top: 4px;
		margin-left: 24px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		line-height: 1.4;
	}

	.result-snippet :global(mark) {
		background: color-mix(in srgb, var(--accent) 25%, transparent);
		color: var(--text-accent);
		border-radius: 2px;
		padding: 0 1px;
	}

	.search-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 18px;
		border-top: 1px solid var(--border-light);
	}

	.search-count {
		font-size: 12px;
		color: var(--text-tertiary);
	}

	.search-hints {
		font-size: 11px;
		color: var(--text-tertiary);
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.search-hints kbd {
		font-size: 11px;
		color: var(--text-tertiary);
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 0px 5px;
		font-family: inherit;
		line-height: 1.4;
	}

	.no-results {
		padding: 32px 20px;
		text-align: center;
		color: var(--text-tertiary);
		font-size: 14px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
	}

	.search-empty {
		padding: 24px 20px;
		text-align: center;
		color: var(--text-tertiary);
		font-size: 13px;
	}
</style>
