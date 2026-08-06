<script lang="ts">
	import { tick } from 'svelte';
	import { activeNote, activeNotePath, appConfig, navHistory } from '$lib/stores/app';
	import { getAllNoteTitles, getQuickAccess } from '$lib/api';
	import { openNoteWindow } from '$lib/utils/window';
	import {
		buildNoteSwitcherSections,
		type NoteSwitcherNote,
		type NoteSwitcherRow,
		type NoteSwitcherSections
	} from '$lib/utils/note-switcher';
	import type { NoteEntry, NoteTitleEntry } from '$lib/types';

	let { onSelectNote = async () => false }: {
		onSelectNote?: (path: string) => Promise<boolean>;
	} = $props();

	let wrapper: HTMLDivElement;
	let trigger: HTMLButtonElement;
	let popover = $state<HTMLDivElement | null>(null);
	let open = $state(false);
	let loading = $state(false);
	let selectingPath = $state<string | null>(null);
	let sections = $state<NoteSwitcherSections>({ recent: [], quickAccess: [] });
	let loadGeneration = 0;

	function normalizedPath(path: string): string {
		return path.replace(/\\/g, '/').replace(/\/$/, '');
	}

	function isInsideVault(path: string, vaultPath: string): boolean {
		return normalizedPath(path).startsWith(`${normalizedPath(vaultPath)}/`);
	}

	function vaultRelativePath(path: string, vaultPath: string): string {
		return normalizedPath(path).slice(normalizedPath(vaultPath).length + 1);
	}

	function joinVaultPath(vaultPath: string, relativePath: string): string {
		const separator = vaultPath.includes('\\') && !vaultPath.includes('/') ? '\\' : '/';
		const root = vaultPath.replace(/[\\/]+$/, '');
		return `${root}${separator}${relativePath.replace(/[\\/]/g, separator)}`;
	}

	function titleEntryToNote(entry: NoteTitleEntry, vaultPath: string): NoteSwitcherNote | null {
		const relativePath = entry.path.replace(/\\/g, '/').replace(/^\/+/, '');
		if (!relativePath || relativePath.split('/').includes('..')) return null;
		const path = joinVaultPath(vaultPath, relativePath);
		if (!isInsideVault(path, vaultPath)) return null;
		return { path, title: entry.title, relativePath };
	}

	function quickAccessEntryToNote(entry: NoteEntry, vaultPath: string): NoteSwitcherNote | null {
		if (!isInsideVault(entry.path, vaultPath)) return null;
		return {
			path: entry.path,
			title: entry.meta.title,
			relativePath: entry.relative_path.replace(/\\/g, '/')
		};
	}

	async function focusInitialRow() {
		await tick();
		if (!open) return;
		const current = wrapper.querySelector<HTMLButtonElement>('.note-primary[aria-current="true"]');
		const first = wrapper.querySelector<HTMLButtonElement>('.note-primary');
		(current ?? first ?? popover)?.focus();
	}

	async function refreshSections(generation: number) {
		const vaultPath = $appConfig?.active_vault;
		if (!vaultPath) {
			sections = { recent: [], quickAccess: [] };
			loading = false;
			await focusInitialRow();
			return;
		}

		const [titlesResult, quickAccessResult] = await Promise.allSettled([
			getAllNoteTitles(),
			getQuickAccess()
		]);
		if (!open || generation !== loadGeneration) return;

		if (titlesResult.status === 'rejected') {
			console.error('Failed to load notes for note switcher:', titlesResult.reason);
		}
		if (quickAccessResult.status === 'rejected') {
			console.error('Failed to load Quick Access for note switcher:', quickAccessResult.reason);
		}

		const knownNotes = titlesResult.status === 'fulfilled'
			? titlesResult.value
				.map((entry) => titleEntryToNote(entry, vaultPath))
				.filter((entry): entry is NoteSwitcherNote => entry !== null)
			: [];
		const currentPath = $activeNotePath;
		if (currentPath && isInsideVault(currentPath, vaultPath)) {
			const currentKey = normalizedPath(currentPath);
			if (!knownNotes.some((note) => normalizedPath(note.path) === currentKey)) {
				knownNotes.push({
					path: currentPath,
					title: $activeNote?.meta.title || currentPath.split(/[\\/]/).pop()?.replace(/\.md$/i, '') || 'Untitled',
					relativePath: vaultRelativePath(currentPath, vaultPath)
				});
			}
		}

		const quickAccessNotes = quickAccessResult.status === 'fulfilled'
			? quickAccessResult.value
				.map((entry) => quickAccessEntryToNote(entry, vaultPath))
				.filter((entry): entry is NoteSwitcherNote => entry !== null)
			: [];

		sections = buildNoteSwitcherSections({
			currentPath,
			historyPaths: $navHistory.stack,
			knownNotes,
			quickAccessNotes
		});
		loading = false;
		await focusInitialRow();
	}

	function toggleSwitcher() {
		if (open) {
			open = false;
			loadGeneration += 1;
			return;
		}
		open = true;
		loading = true;
		selectingPath = null;
		sections = { recent: [], quickAccess: [] };
		loadGeneration += 1;
		void refreshSections(loadGeneration);
	}

	async function closeSwitcher(restoreTriggerFocus: boolean) {
		open = false;
		loadGeneration += 1;
		if (restoreTriggerFocus) {
			await tick();
			trigger?.focus();
		}
	}

	async function selectNote(row: NoteSwitcherRow) {
		if (selectingPath !== null) return;
		selectingPath = row.path;
		try {
			if (await onSelectNote(row.path)) await closeSwitcher(false);
		} finally {
			selectingPath = null;
		}
	}

	function handlePopoverKeydown(event: KeyboardEvent) {
		if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
		const buttons = Array.from(wrapper.querySelectorAll<HTMLButtonElement>('.note-primary:not(:disabled)'));
		if (buttons.length === 0) return;
		event.preventDefault();
		const activeRow = (document.activeElement as HTMLElement | null)?.closest('.note-row');
		const activeButton = activeRow?.querySelector<HTMLButtonElement>('.note-primary') ?? null;
		const currentIndex = activeButton ? buttons.indexOf(activeButton) : -1;
		let nextIndex: number;
		if (event.key === 'Home') nextIndex = 0;
		else if (event.key === 'End') nextIndex = buttons.length - 1;
		else if (event.key === 'ArrowDown') nextIndex = currentIndex < 0 ? 0 : (currentIndex + 1) % buttons.length;
		else nextIndex = currentIndex < 0 ? buttons.length - 1 : (currentIndex - 1 + buttons.length) % buttons.length;
		buttons[nextIndex].focus();
	}

	$effect(() => {
		if (!open) return;
		const handleOutsideMouseDown = (event: MouseEvent) => {
			if (!wrapper.contains(event.target as Node)) void closeSwitcher(false);
		};
		const handleEscape = (event: KeyboardEvent) => {
			if (event.key !== 'Escape') return;
			event.preventDefault();
			event.stopPropagation();
			void closeSwitcher(true);
		};
		document.addEventListener('mousedown', handleOutsideMouseDown, true);
		document.addEventListener('keydown', handleEscape, true);
		return () => {
			document.removeEventListener('mousedown', handleOutsideMouseDown, true);
			document.removeEventListener('keydown', handleEscape, true);
		};
	});
</script>

<div class="note-switcher" bind:this={wrapper}>
	<button
		bind:this={trigger}
		class="switcher-trigger"
		type="button"
		aria-label="Switch note"
		aria-haspopup="dialog"
		aria-expanded={open}
		onclick={toggleSwitcher}
	>
		<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
			<rect x="7" y="4" width="12" height="14" rx="2" />
			<path d="M5 7H4a2 2 0 00-2 2v9a2 2 0 002 2h9a2 2 0 002-2v-1" />
		</svg>
	</button>

	{#if open}
		<div
			bind:this={popover}
			class="switcher-popover"
			role="dialog"
			aria-label="Note switcher"
			tabindex="-1"
			onkeydown={handlePopoverKeydown}
		>
			{#if loading}
				<div class="switcher-status">Loading notes…</div>
			{:else if sections.recent.length === 0 && sections.quickAccess.length === 0}
				<div class="switcher-status">No notes available</div>
			{:else}
				{#each [
					{ label: 'Recent', rows: sections.recent },
					{ label: 'Quick Access', rows: sections.quickAccess }
				] as section (section.label)}
					{#if section.rows.length > 0}
						<section class="switcher-section" aria-labelledby={`note-switcher-${section.label.replace(' ', '-').toLowerCase()}`}>
							<h2 id={`note-switcher-${section.label.replace(' ', '-').toLowerCase()}`}>{section.label}</h2>
							{#each section.rows as row (row.path)}
								<div class="note-row" class:current-row={row.current}>
									{#if row.current}<span class="current-marker" aria-hidden="true"></span>{/if}
									<button
										class="note-primary"
										type="button"
										aria-current={row.current ? 'true' : undefined}
										disabled={selectingPath !== null}
										onclick={() => selectNote(row)}
									>
										<span class="note-title">{row.title}</span>
										<span class="note-folder">{row.folder}</span>
									</button>
									<button
										class="open-window"
										type="button"
										aria-label={`Open ${row.title} in new window`}
										title={`Open ${row.title} in new window`}
										onclick={() => openNoteWindow(row.path, row.title)}
									>
										<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
											<rect x="4" y="4" width="13" height="13" rx="2" />
											<path d="M10 14L20 4m-6 0h6v6" />
										</svg>
									</button>
								</div>
							{/each}
						</section>
					{/if}
				{/each}
			{/if}
		</div>
	{/if}
</div>

<style>
	.note-switcher {
		position: relative;
		-webkit-app-region: no-drag;
	}

	.switcher-trigger {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		padding: 0;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--text-secondary);
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}

	.switcher-trigger:hover,
	.switcher-trigger[aria-expanded='true'] {
		background: var(--bg-tertiary, var(--bg-hover));
		color: var(--text-primary);
	}

	.switcher-trigger:focus-visible,
	.note-primary:focus-visible,
	.open-window:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: -2px;
	}

	.switcher-popover {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 1000;
		width: min(320px, calc(100vw - 24px));
		max-height: min(420px, calc(100vh - 58px));
		overflow-y: auto;
		padding: 6px;
		border: 1px solid var(--border-color);
		border-radius: 10px;
		background: var(--bg-secondary);
		box-shadow: 0 12px 30px rgba(0, 0, 0, 0.2);
		color: var(--text-primary);
		user-select: none;
		-webkit-app-region: no-drag;
	}

	.switcher-section + .switcher-section {
		margin-top: 5px;
		padding-top: 5px;
		border-top: 1px solid var(--border-color);
	}

	h2 {
		margin: 0;
		padding: 5px 8px 4px;
		color: var(--text-tertiary);
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.06em;
		line-height: 1.2;
		text-transform: uppercase;
	}

	.note-row {
		position: relative;
		display: flex;
		align-items: center;
		min-height: 42px;
		border-radius: 7px;
	}

	.note-row:hover,
	.note-row:focus-within {
		background: var(--bg-hover);
	}

	.note-row.current-row {
		background: color-mix(in srgb, var(--accent) 9%, transparent);
	}

	.current-marker {
		position: absolute;
		left: 7px;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--accent);
	}

	.note-primary {
		display: flex;
		min-width: 0;
		flex: 1;
		flex-direction: column;
		align-items: flex-start;
		gap: 2px;
		padding: 6px 38px 6px 12px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}

	.current-row .note-primary {
		padding-left: 18px;
	}

	.note-primary:disabled {
		cursor: wait;
	}

	.note-title,
	.note-folder {
		display: block;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.note-title {
		font-size: 12px;
		font-weight: 500;
		line-height: 1.25;
	}

	.note-folder {
		color: var(--text-tertiary);
		font-size: 10.5px;
		line-height: 1.2;
	}

	.open-window {
		position: absolute;
		right: 5px;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		padding: 0;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--text-tertiary);
		cursor: pointer;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.12s, background 0.12s, color 0.12s;
	}

	.note-row:hover .open-window,
	.note-row:focus-within .open-window {
		opacity: 1;
		pointer-events: auto;
	}

	.open-window:hover {
		background: var(--bg-tertiary, var(--bg-hover));
		color: var(--text-primary);
	}

	.switcher-status {
		padding: 22px 12px;
		color: var(--text-tertiary);
		font-size: 11px;
		text-align: center;
	}
</style>
