<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import Editor from './Editor.svelte';
	import {
		activeNote,
		activeNotePath,
		editorDirty,
		readOnly,
		sourceMode,
		theme
	} from '$lib/stores/app';
	import { readNote } from '$lib/api';
	import { keybindings, matchAction } from '$lib/keybindings';
	import type { FileEvent } from '$lib/types';

	let { notePath }: { notePath: string } = $props();

	import { darkThemes } from '$lib/platform';

	const appWindow = getCurrentWindow();
	const isMac = navigator.platform.startsWith('Mac');
	let editor = $state<Editor>(null!);
	let unlistenFileChange: (() => void) | null = null;
	let maximized = $state(false);
	let loadError = $state<string | null>(null);

	async function checkMaximized() {
		maximized = await appWindow.isMaximized();
	}

	$effect(() => {
		checkMaximized();
		const unlisten = appWindow.onResized(() => checkMaximized());
		return () => { unlisten.then(fn => fn()); };
	});

	$effect(() => {
		const t = $theme || 'system';
		const namedThemes = ['solarized-light', 'solarized-dark', 'catppuccin', 'nord', 'tokyo-night', 'github-light', 'github-dark', 'dracula', 'blueberry', 'forest-green', 'gruvbox', 'midnight-tide', 'cherry-blossom', 'synthwave', 'ember', 'moonlit', 'light-coffee', 'dark-coffee', 'cotton-candy', 'crimson', 'cloud', 'peach', 'material-dark', 'material-light', 'monokai', 'rose-pine', 'everforest', 'horizon', 'cyberpunk', 'black', 'one-dark'];
		const root = document.documentElement;
		root.classList.remove('dark');
		root.removeAttribute('data-theme');
		if (namedThemes.includes(t)) {
			root.setAttribute('data-theme', t);
			if (darkThemes.includes(t)) root.classList.add('dark');
		} else if (t === 'dark' || (t === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)) {
			root.classList.add('dark');
		}
	});

	let lastMouseDown = 0;
	const RESIZE_EDGE = 6;

	function handleMouseDown(e: MouseEvent) {
		if (e.button !== 0) return;
		const target = e.target as HTMLElement;
		if (target.closest('.titlebar-controls') || target.closest('.titlebar-actions')) return;

		if (!maximized) {
			const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
			if (e.clientY - rect.top < RESIZE_EDGE || e.clientX - rect.left < RESIZE_EDGE) return;
		}

		const now = Date.now();
		if (now - lastMouseDown < 300) {
			appWindow.toggleMaximize();
			lastMouseDown = 0;
			return;
		}
		lastMouseDown = now;
		appWindow.startDragging();
	}

	function handleKeydown(e: KeyboardEvent) {
		const action = matchAction(e, $keybindings);
		if (action === 'save') {
			e.preventDefault();
			editor?.forceSave();
		} else if (action === 'toggle-source') {
			e.preventDefault();
			$sourceMode = !$sourceMode;
		}
	}

	onMount(async () => {
		try {
			const content = await readNote(notePath);
			$activeNote = content;
			$activeNotePath = notePath;
			$editorDirty = false;
			setTimeout(() => {
				editor?.loadNote(notePath, content.content);
				appWindow.setTitle(`${content.meta.title} - HelixNotes`);
			}, 50);
		} catch (e) {
			loadError = String(e);
		}

		unlistenFileChange = await listen<FileEvent>('file-changed', async (event) => {
			if (event.payload.path === notePath && event.payload.event_type === 'modify') {
				if (!$editorDirty) {
					try {
						const content = await readNote(notePath);
						// Ignore the file-watcher echo of our own save; only reload genuine external edits.
						if (content.content.trim() !== (editor?.getCurrentBody() ?? '').trim()) {
							$activeNote = content;
							editor?.loadNote(notePath, content.content);
						}
					} catch (_) {}
				}
			}
		});
	});

	onDestroy(() => {
		unlistenFileChange?.();
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="note-window">
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="nw-titlebar" class:macos={isMac} onmousedown={handleMouseDown}>
		<div class="nw-titlebar-brand">
			<svg width="14" height="14" viewBox="0 0 48 48" fill="none">
				<rect width="48" height="48" rx="12" fill="var(--accent)" />
				<circle cx="16" cy="16" r="3.5" fill="white" opacity="0.9" />
				<circle cx="32" cy="16" r="3.5" fill="white" opacity="0.9" />
				<circle cx="16" cy="32" r="3.5" fill="white" opacity="0.9" />
				<circle cx="32" cy="32" r="3.5" fill="white" opacity="0.9" />
				<line x1="19" y1="18" x2="29" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
				<line x1="29" y1="18" x2="19" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
			</svg>
			<span class="nw-title">{$activeNote?.meta.title || 'Loading...'}</span>
			{#if $editorDirty}
				<span class="nw-dirty-dot" title="Unsaved changes"></span>
			{/if}
		</div>
		<div class="titlebar-actions">
			<button class="nw-btn" class:active={$sourceMode} onclick={() => ($sourceMode = !$sourceMode)} title="Toggle Source Mode">
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" />
				</svg>
			</button>
			<button class="nw-btn" class:active={$readOnly} onclick={() => ($readOnly = !$readOnly)} title={$readOnly ? 'Switch to Edit Mode' : 'Switch to View Mode'}>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					{#if $readOnly}
						<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
						<circle cx="12" cy="12" r="3" />
					{:else}
						<path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94" />
						<path d="M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19" />
						<line x1="1" y1="1" x2="23" y2="23" />
					{/if}
				</svg>
			</button>
		</div>
		{#if !isMac}
		<div class="titlebar-controls">
			<button class="titlebar-btn" onclick={() => appWindow.minimize()} title="Minimize">
				<svg width="10" height="10" viewBox="0 0 10 10">
					<line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
				</svg>
			</button>
			<button class="titlebar-btn" onclick={() => appWindow.toggleMaximize()} title={maximized ? 'Restore' : 'Maximize'}>
				{#if maximized}
					<svg width="10" height="10" viewBox="0 0 10 10">
						<rect x="2.5" y="0.5" width="7" height="7" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" />
						<rect x="0.5" y="2.5" width="7" height="7" rx="1" fill="var(--bg-secondary)" stroke="currentColor" stroke-width="1.2" />
					</svg>
				{:else}
					<svg width="10" height="10" viewBox="0 0 10 10">
						<rect x="1" y="1" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2" />
					</svg>
				{/if}
			</button>
			<button class="titlebar-btn titlebar-close" onclick={() => appWindow.close()} title="Close">
				<svg width="10" height="10" viewBox="0 0 10 10">
					<line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
					<line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
				</svg>
			</button>
		</div>
		{/if}
	</div>

	{#if loadError}
		<div class="nw-error">
			<p>Failed to load note</p>
			<p class="nw-error-detail">{loadError}</p>
		</div>
	{:else}
		<div class="nw-editor">
			<Editor bind:this={editor} />
		</div>
	{/if}
</div>

<style>
	.note-window {
		display: flex;
		flex-direction: column;
		height: 100vh;
		background: var(--bg-primary);
	}

	.nw-titlebar {
		display: flex;
		align-items: center;
		height: 34px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-color);
		user-select: none;
		flex-shrink: 0;
		-webkit-app-region: drag;
	}

	.nw-titlebar.macos {
		padding-left: 78px;
	}

	.nw-titlebar-brand {
		display: flex;
		align-items: center;
		gap: 8px;
		padding-left: 14px;
		pointer-events: none;
		flex: 1;
		min-width: 0;
	}

	.nw-title {
		font-size: 12px;
		font-weight: 500;
		color: var(--text-tertiary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.nw-dirty-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--accent);
		flex-shrink: 0;
	}

	.titlebar-actions {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-right: 8px;
		-webkit-app-region: no-drag;
	}

	.nw-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: 7px;
		background: var(--bg-hover);
		color: var(--text-secondary);
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}

	.nw-btn:hover {
		background: var(--bg-tertiary, var(--bg-hover));
		color: var(--text-primary);
	}

	.nw-btn.active {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
		color: var(--accent);
	}

	.titlebar-controls {
		display: flex;
		height: 100%;
		-webkit-app-region: no-drag;
	}

	.titlebar-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 38px;
		height: 100%;
		border: none;
		background: none;
		color: var(--text-tertiary);
		cursor: pointer;
		transition: background 0.1s, color 0.1s;
	}

	.titlebar-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.titlebar-close:hover {
		background: #e81123;
		color: white;
	}

	.nw-editor {
		flex: 1;
		overflow: hidden;
	}

	.nw-error {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		color: var(--text-secondary);
		font-size: 14px;
	}

	.nw-error-detail {
		font-size: 12px;
		color: var(--text-tertiary);
		max-width: 400px;
		text-align: center;
		word-break: break-word;
	}
</style>
