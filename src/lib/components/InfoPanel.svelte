<script lang="ts">
	import { showInfo, appConfig, activeNotePath } from '$lib/stores/app';
	import { getVaultStats, findOrphanedAttachments, trashOrphanedAttachments } from '$lib/api';
	import type { OrphanAttachment } from '$lib/api';
	import { openUrl } from '$lib/api';
	import { getVersion } from '@tauri-apps/api/app';
	import type { VaultStats } from '$lib/types';
	import {
		ACTIONS,
		keybindings,
		setBinding,
		resetBinding,
		bindingFromEvent,
		bindingToKeys,
		bindingsEqual,
		type ActionDef,
	} from '$lib/keybindings';
	import { isMobile } from '$lib/platform';

	const modKey = navigator.platform.startsWith('Mac') ? '⌘' : 'Ctrl';

	// Customizable shortcuts, grouped for display.
	const actionGroups: { title: string; actions: ActionDef[] }[] = [
		{ title: 'General', actions: ACTIONS.filter((a) => a.group === 'General') },
		{ title: 'Interface', actions: ACTIONS.filter((a) => a.group === 'Interface') },
		{ title: 'Navigation', actions: ACTIONS.filter((a) => a.group === 'Navigation') },
	];

	// id of the action currently listening for a new key combo, or null.
	let capturingId = $state<string | null>(null);
	// id of an action whose row should briefly flash a "already in use" warning.
	let conflictId = $state<string | null>(null);
	let conflictTimer: ReturnType<typeof setTimeout> | null = null;

	function startCapture(id: string) {
		conflictId = null;
		capturingId = id;
	}

	function stopCapture() {
		capturingId = null;
	}

	function flashConflict(id: string) {
		conflictId = id;
		if (conflictTimer) clearTimeout(conflictTimer);
		conflictTimer = setTimeout(() => { conflictId = null; }, 1600);
	}

	function captureKeydown(e: KeyboardEvent) {
		if (!capturingId) return;
		// Capture phase + stopImmediatePropagation keeps the combo from also
		// triggering the app-level shortcut handler on window.
		e.preventDefault();
		e.stopImmediatePropagation();
		if (e.key === 'Escape') { stopCapture(); return; }

		const binding = bindingFromEvent(e);
		if (!binding) return; // modifier-only press; keep listening

		// Reject combos already bound to a different action.
		const map = $keybindings;
		const target = capturingId;
		const clash = Object.keys(map).find((other) => other !== target && bindingsEqual(map[other], binding));
		if (clash) {
			flashConflict(target);
			stopCapture();
			return;
		}

		setBinding(target, binding);
		stopCapture();
	}

	$effect(() => {
		if (!capturingId) return;
		window.addEventListener('keydown', captureKeydown, true);
		return () => window.removeEventListener('keydown', captureKeydown, true);
	});

	function resetKey(e: MouseEvent, id: string) {
		e.preventDefault();
		resetBinding(id);
		if (capturingId === id) stopCapture();
	}

	let stats = $state<VaultStats | null>(null);
	let activeTab = $state<'about' | 'shortcuts'>(isMobile ? 'about' : 'shortcuts');
	let appVersion = $state('...');
	let orphans = $state<OrphanAttachment[] | null>(null);
	let scanning = $state(false);
	let trashing = $state(false);
	let locationCopyState = $state<'idle' | 'copied' | 'error'>('idle');
	let locationCopyTimer: ReturnType<typeof setTimeout> | null = null;
	const orphanTotal = $derived((orphans ?? []).reduce((a, o) => a + o.size, 0));
	const diskLocation = $derived($activeNotePath ?? $appConfig?.active_vault ?? null);

	getVersion().then(v => appVersion = v).catch(() => appVersion = '0.0.0');

	function close() {
		if (locationCopyTimer) clearTimeout(locationCopyTimer);
		locationCopyTimer = null;
		locationCopyState = 'idle';
		$showInfo = false;
		activeTab = isMobile ? 'about' : 'shortcuts';
	}

	function openLink(url: string) {
		openUrl(url).catch(console.error);
	}

	async function copyDiskLocation() {
		if (!diskLocation) return;

		if (locationCopyTimer) clearTimeout(locationCopyTimer);
		try {
			if (!navigator.clipboard?.writeText) throw new Error('Clipboard API unavailable');
			await navigator.clipboard.writeText(diskLocation);
			locationCopyState = 'copied';
		} catch (e) {
			console.error('Failed to copy disk location:', e);
			locationCopyState = 'error';
		}
		locationCopyTimer = setTimeout(() => {
			locationCopyState = 'idle';
			locationCopyTimer = null;
		}, 1600);
	}

	function formatSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
	}

	$effect(() => {
		if ($showInfo) {
			getVaultStats().then((s) => { stats = s; }).catch(console.error);
		} else {
			stats = null;
			orphans = null;
		}
	});

	async function scanOrphans() {
		scanning = true;
		try { orphans = await findOrphanedAttachments(); }
		catch (e) { console.error(e); }
		finally { scanning = false; }
	}
	async function trashOrphans() {
		if (!orphans || orphans.length === 0) return;
		trashing = true;
		try {
			await trashOrphanedAttachments(orphans.map((o) => o.name));
			orphans = null;
			getVaultStats().then((sv) => { stats = sv; }).catch(console.error);
		} catch (e) { console.error(e); }
		finally { trashing = false; }
	}
</script>

{#if $showInfo}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="info-overlay" onclick={close} onkeydown={(e) => { if (e.key === 'Escape') close(); }}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="info-panel" onclick={(e) => e.stopPropagation()}>
			<div class="info-header">
				<h2>Info</h2>
				<button class="close-btn" onclick={close} aria-label="Close info">
					<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<line x1="18" y1="6" x2="6" y2="18" />
						<line x1="6" y1="6" x2="18" y2="18" />
					</svg>
				</button>
			</div>

			<div class="info-body">
				{#if !isMobile}
				<div class="info-tabs">
					<button class="info-tab" class:active={activeTab === 'shortcuts'} onclick={() => activeTab = 'shortcuts'}>Shortcuts</button>
					<button class="info-tab" class:active={activeTab === 'about'} onclick={() => activeTab = 'about'}>About</button>
				</div>
				{/if}

				{#if activeTab === 'about'}
					<div class="info-logo">
						<svg width="48" height="48" viewBox="0 0 48 48" fill="none">
							<rect width="48" height="48" rx="12" fill="var(--accent)" />
							<circle cx="16" cy="16" r="3.5" fill="white" opacity="0.9" />
							<circle cx="32" cy="16" r="3.5" fill="white" opacity="0.9" />
							<circle cx="16" cy="32" r="3.5" fill="white" opacity="0.9" />
							<circle cx="32" cy="32" r="3.5" fill="white" opacity="0.9" />
							<line x1="19" y1="18" x2="29" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
							<line x1="29" y1="18" x2="19" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
						</svg>
					</div>
					<h3 class="app-name">HelixNotes</h3>
					<p class="app-version">v{appVersion}</p>
					<p class="app-description">A local markdown note-taking app.</p>

					{#if diskLocation}
						<div class="disk-location">
							<div class="disk-location-header">
								<span class="disk-location-label">Disk location</span>
								<button
									class="copy-location-btn"
									class:copied={locationCopyState === 'copied'}
									class:error={locationCopyState === 'error'}
									onclick={copyDiskLocation}
									aria-live="polite"
									title="Copy disk location"
								>
									{#if locationCopyState === 'copied'}
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 4 4L19 6" /></svg>
										Copied
									{:else if locationCopyState === 'error'}
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
										Copy failed
									{:else}
										<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M15 9V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h3" /></svg>
										Copy
									{/if}
								</button>
							</div>
							<code class="disk-location-path" title={diskLocation}>{diskLocation}</code>
						</div>
					{/if}

					{#if stats}
						<div class="info-stats">
							<div class="stat-row">
								<span class="stat-label">Notes</span>
								<span class="stat-value">{stats.total_notes}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Attachments</span>
								<span class="stat-value">{stats.total_attachments}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Notes size</span>
								<span class="stat-value">{formatSize(stats.notes_size)}</span>
							</div>
							<div class="stat-row">
								<span class="stat-label">Attachments size</span>
								<span class="stat-value">{formatSize(stats.attachments_size)}</span>
							</div>
							<div class="stat-row stat-total">
								<span class="stat-label">Total vault size</span>
								<span class="stat-value">{formatSize(stats.total_size)}</span>
							</div>
						</div>
					{/if}

					{#if stats}
						<div class="info-cleanup">
							{#if orphans === null}
								<button class="cleanup-btn" onclick={scanOrphans} disabled={scanning}>{#if scanning}<svg class="spinner-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" opacity="0.25" /><path d="M12 2a10 10 0 019.95 9" /></svg>Scanning…{:else}Find orphaned attachments{/if}</button>
							{:else if orphans.length === 0}
								<p class="cleanup-msg">No orphaned attachments. Nothing to clean up.</p>
							{:else}
								<p class="cleanup-msg">{orphans.length} {orphans.length === 1 ? 'attachment is' : 'attachments are'} not referenced by any note ({formatSize(orphanTotal)}).</p>
								<div class="cleanup-list">
									{#each orphans as o}
										<div class="cleanup-row"><span class="cleanup-name" title={o.name}>{o.name}</span><span class="cleanup-size">{formatSize(o.size)}</span></div>
									{/each}
								</div>
								<button class="cleanup-btn" onclick={trashOrphans} disabled={trashing}>{#if trashing}<svg class="spinner-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" opacity="0.25" /><path d="M12 2a10 10 0 019.95 9" /></svg>Moving…{:else}Move {orphans.length} to Trash{/if}</button>
								<p class="cleanup-hint">Moved to the vault's trash folder (recoverable), not permanently deleted.</p>
							{/if}
						</div>
					{/if}

					<div class="info-credits">
						<p>Created by <strong>Yuri Karamian</strong></p>
						<button class="info-link" onclick={() => openLink('https://helixnotes.com')}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
								<circle cx="12" cy="12" r="10" />
								<line x1="2" y1="12" x2="22" y2="12" />
								<path d="M12 2a15.3 15.3 0 014 10 15.3 15.3 0 01-4 10 15.3 15.3 0 01-4-10 15.3 15.3 0 014-10z" />
							</svg>
							helixnotes.com
						</button>
					</div>
				{:else}
					<div class="shortcuts-section">
						{#if !isMobile}
							<p class="shortcuts-hint">Click a shortcut to rebind it, right-click to reset.</p>
						{/if}
						{#each actionGroups as group}
							<h4 class="shortcuts-group-title">{group.title}</h4>
							{#each group.actions as action}
								<div class="shortcut-row">
									<span class="shortcut-desc">{action.label}</span>
									{#if isMobile}
										<span class="shortcut-keys">{#each bindingToKeys($keybindings[action.id]) as key, i}{#if i > 0}+{/if}<kbd>{key}</kbd>{/each}</span>
									{:else}
										<button
											class="shortcut-key-btn"
											class:capturing={capturingId === action.id}
											class:conflict={conflictId === action.id}
											title="Click to rebind · right-click to reset"
											onclick={() => startCapture(action.id)}
											oncontextmenu={(e) => resetKey(e, action.id)}
										>
											{#if capturingId === action.id}
												<span class="capture-prompt">Press keys…</span>
											{:else if conflictId === action.id}
												<span class="conflict-msg">Already in use</span>
											{:else}
												<span class="shortcut-keys">{#each bindingToKeys($keybindings[action.id]) as key, i}{#if i > 0}+{/if}<kbd>{key}</kbd>{/each}</span>
											{/if}
										</button>
									{/if}
								</div>
							{/each}
						{/each}
						<div class="shortcut-row"><span class="shortcut-desc">Close panel / exit focus</span><span class="shortcut-keys"><kbd>Esc</kbd></span></div>

						<h4 class="shortcuts-group-title">Formatting</h4>
						<div class="shortcut-row"><span class="shortcut-desc">Bold</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>B</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Italic</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>I</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Underline</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>U</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Strikethrough</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>Shift</kbd>+<kbd>X</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Code</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>E</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Link</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>K</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Undo</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>Z</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Redo</span><span class="shortcut-keys"><kbd>{modKey}</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Move line up / down</span><span class="shortcut-keys"><kbd>Alt</kbd>+<kbd>↑↓</kbd></span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Move list item up / down</span><span class="shortcut-keys"><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>↑↓</kbd></span></div>

						<h4 class="shortcuts-group-title">Editor Commands</h4>
						<div class="shortcut-row"><span class="shortcut-desc">Slash commands</span><span class="shortcut-keys"><kbd>/</kbd></span></div>
						<div class="shortcut-row command-detail"><span class="shortcut-desc">Headings, lists, code block, table, blockquote, collapsible section, horizontal rule</span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Wiki-link to note</span><span class="shortcut-keys"><kbd>[[</kbd></span></div>
						<div class="shortcut-row command-detail"><span class="shortcut-desc">Type <kbd>[[</kbd> to search and link to another note. Close with <kbd>]]</kbd> or pick from the list.</span></div>

						<h4 class="shortcuts-group-title">Editor Features</h4>
						<div class="shortcut-row"><span class="shortcut-desc">Right-click in editor for formatting menu</span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Right-click a table cell for table options</span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Right-click a link to open, copy, edit, or remove</span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Click an image to resize (small / medium / full)</span></div>
						<div class="shortcut-row"><span class="shortcut-desc">Drag & drop images, PDFs, or files into the editor</span></div>
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.info-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.35);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 2000;
	}

	.info-panel {
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 16px;
		box-shadow: var(--shadow-lg);
		width: 500px;
		height: 80vh;
		max-height: 600px;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.info-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 20px 24px 16px;
		border-bottom: 1px solid var(--border-light);
	}

	.info-header h2 {
		font-size: 18px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 4px;
		border-radius: 6px;
		display: flex;
		align-items: center;
	}

	.close-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.info-body {
		padding: 0 24px 16px;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 4px;
		overflow-y: auto;
	}

	.info-tabs {
		display: flex;
		gap: 2px;
		width: 100%;
		padding: 16px 0 12px;
		background: var(--bg-primary);
		position: sticky;
		top: 0;
		z-index: 1;
	}

	.info-tab {
		flex: 1;
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-tertiary);
		font-size: 13px;
		font-weight: 500;
		padding: 6px 8px;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}

	.info-tab:hover {
		color: var(--text-secondary);
	}

	.info-tab.active {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}

	.info-logo {
		margin-top: 12px;
		margin-bottom: 8px;
	}

	.app-name {
		font-size: 20px;
		font-weight: 700;
		color: var(--text-primary);
	}

	.app-version {
		font-size: 12px;
		color: var(--text-tertiary);
	}

	.app-description {
		font-size: 13px;
		color: var(--text-secondary);
		margin-top: 4px;
	}

	.disk-location {
		width: 100%;
		margin-top: 18px;
		padding: 11px 14px 12px;
		border: 1px solid var(--border-light);
		border-radius: 10px;
		background: var(--bg-secondary);
		text-align: left;
	}

	.disk-location-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.disk-location-label {
		color: var(--text-tertiary);
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	.disk-location-path {
		display: block;
		margin-top: 7px;
		color: var(--text-primary);
		font-family: "JetBrains Mono", ui-monospace, SFMono-Regular, Consolas, monospace;
		font-size: 11px;
		line-height: 1.45;
		overflow-wrap: anywhere;
	}

	.copy-location-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin: -4px -6px -4px 0;
		padding: 4px 6px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--accent);
		font-size: 11px;
		font-weight: 600;
		cursor: pointer;
	}

	.copy-location-btn:hover {
		background: var(--accent-light);
	}

	.copy-location-btn.copied {
		color: var(--success);
	}

	.copy-location-btn.error {
		color: var(--danger);
	}

	.disk-location + .info-stats {
		margin-top: 12px;
	}

	.info-stats {
		width: 100%;
		margin-top: 20px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-light);
		border-radius: 10px;
		padding: 4px 0;
	}

	.stat-row {
		display: flex;
		justify-content: space-between;
		padding: 7px 16px;
	}

	.stat-label {
		font-size: 13px;
		color: var(--text-secondary);
	}

	.stat-value {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.stat-total {
		border-top: 1px solid var(--border-light);
	}

	.stat-total .stat-value {
		color: var(--accent);
	}

	.info-credits {
		margin-top: 20px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}

	.info-credits p {
		font-size: 13px;
		color: var(--text-secondary);
	}

	.info-link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		color: var(--accent);
		font-size: 13px;
		cursor: pointer;
		padding: 4px 10px;
		border-radius: 6px;
		transition: background 0.15s;
	}

	.info-link:hover {
		background: var(--accent-light);
	}
	.info-cleanup {
		width: 100%;
		margin-top: 16px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}
	.cleanup-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		color: var(--text-primary);
		font-size: 13px;
		padding: 7px 14px;
		cursor: pointer;
	}
	.cleanup-btn:hover:not(:disabled) { background: var(--bg-hover); }
	.cleanup-btn:disabled { opacity: 0.6; cursor: default; }
	.cleanup-msg {
		font-size: 13px;
		color: var(--text-secondary);
		text-align: center;
	}
	.cleanup-list {
		width: 100%;
		max-height: 140px;
		overflow-y: auto;
		background: var(--bg-secondary);
		border: 1px solid var(--border-light);
		border-radius: 8px;
	}
	.cleanup-row {
		display: flex;
		justify-content: space-between;
		gap: 12px;
		padding: 5px 12px;
		font-size: 12px;
	}
	.cleanup-name {
		color: var(--text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.cleanup-size {
		color: var(--text-tertiary);
		flex-shrink: 0;
	}
	.cleanup-hint {
		font-size: 11px;
		color: var(--text-tertiary);
		text-align: center;
	}
	.spinner-icon {
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.shortcuts-section {
		width: 100%;
		text-align: left;
		padding-top: 4px;
	}

	.shortcuts-group-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: var(--text-tertiary);
		margin: 16px 0 8px;
		padding-bottom: 4px;
		border-bottom: 1px solid var(--border-light);
	}

	.shortcuts-group-title:first-child {
		margin-top: 0;
	}

	.shortcut-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 5px 0;
		gap: 12px;
	}

	.shortcut-desc {
		font-size: 13px;
		color: var(--text-secondary);
	}

	.shortcut-keys {
		display: flex;
		align-items: center;
		gap: 3px;
		flex-shrink: 0;
		font-size: 12px;
		color: var(--text-tertiary);
	}

	.shortcuts-hint {
		font-size: 12px;
		color: var(--text-tertiary);
		margin: 0 0 4px;
	}

	.shortcut-key-btn {
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		padding: 2px 4px;
		margin: -2px -4px;
		cursor: pointer;
		display: flex;
		align-items: center;
		flex-shrink: 0;
		font: inherit;
	}

	.shortcut-key-btn:hover {
		background: var(--bg-hover);
	}

	.shortcut-key-btn.capturing {
		border-color: var(--accent);
		background: var(--accent-light);
	}

	.shortcut-key-btn.conflict {
		border-color: var(--danger, #e11d48);
	}

	.capture-prompt {
		font-size: 12px;
		color: var(--accent);
	}

	.conflict-msg {
		font-size: 12px;
		color: var(--danger, #e11d48);
	}

	.shortcut-keys kbd,
	.shortcut-desc kbd {
		display: inline-block;
		background: var(--bg-secondary);
		border: 1px solid var(--border-color);
		border-radius: 4px;
		padding: 1px 5px;
		font-size: 11px;
		font-family: inherit;
		color: var(--text-primary);
		line-height: 1.4;
	}

	.command-detail {
		padding: 0 0 4px 8px;
	}

	.command-detail .shortcut-desc {
		font-size: 12px;
		color: var(--text-tertiary);
		line-height: 1.4;
	}

	/* Mobile */
	@media (max-width: 600px) {
		.info-panel {
			width: 100%;
			height: 100%;
			max-height: 100%;
			border-radius: 0;
			border: none;
		}

		.info-header {
			padding-top: calc(env(safe-area-inset-top, 12px) + 12px);
		}

		.copy-location-btn {
			min-height: 44px;
			margin-block: -10px;
			padding-inline: 10px;
		}
	}
</style>
