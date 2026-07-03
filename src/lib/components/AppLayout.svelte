<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import Sidebar from './Sidebar.svelte';
	import NoteList from './NoteList.svelte';
	import Editor from './Editor.svelte';
	import SearchPanel from './SearchPanel.svelte';
	import CommandPalette from './CommandPalette.svelte';
	import SettingsPanel from './SettingsPanel.svelte';
	import InfoPanel from './InfoPanel.svelte';
	import TitleBar from './TitleBar.svelte';
	import ResizeHandle from './ResizeHandle.svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		sidebarWidth,
		notelistWidth,
		outlineWidth,
		sidebarCollapsed,
		notelistCollapsed,
		collapsedNotebooks,
		showSearch,
		showCommandPalette,
		theme,
		customThemes,
		focusMode,
		readOnly,
		activeNote,
		activeNotePath,
		activeNotebook,
		editorDirty,
		showInfo,
		showSettings,
		sourceMode,
		mobileView,
		appConfig,
		quickAccessPaths,
		tags,
		notes,
		notebooks,
		rootNoteCount,
		viewMode,
		sortMode,
		groupNotesByDate,
		tasksLayout,
		tasksHideCompleted,
		tasksOnlyFlagged,
		tasksSort,
		activeTag,
		updateAvailable as globalUpdateAvailable,
		settingsTab,
		navHistory,
		viewerNote,
		notebookSortMode,
		notebookOrder,
		noteOrder,
		syncState,
		platformIsMobile,
		activeVaultConfig,
	} from '$lib/stores/app';
	import { keybindings, matchAction } from '$lib/keybindings';

	const appWindow = getCurrentWindow();
	const isMac = navigator.platform.startsWith('Mac');
	const isMobile = $derived($platformIsMobile);
	import { loadVaultState, saveVaultState, readNote, createDailyNote, createBackup, getPendingOpenFile, addQuickAccess, removeQuickAccess, getQuickAccess, setTheme, syncNow, setTaskDone, setTaskPriority, setTaskDue, findOrphanedAttachments, trashOrphanedAttachments } from '$lib/api';
	import { applyTheme, isAndroid } from '$lib/platform';
	import { debounce } from '$lib/utils/debounce';
	import { openNoteWindow } from '$lib/utils/window';
	import { get } from 'svelte/store';
	import type { VaultState, FileEvent, NotebookEntry, TaskItem } from '$lib/types';

	function findNotebookByPath(list: NotebookEntry[], relPath: string): NotebookEntry | null {
		for (const nb of list) {
			if (nb.relative_path === relPath) return nb;
			const found = findNotebookByPath(nb.children, relPath);
			if (found) return found;
		}
		return null;
	}

	let sidebar: Sidebar;
	let noteList: NoteList;
	let editor: Editor;
	let unlistenFileChange: (() => void) | null = null;
	let unlistenOpenFile: (() => void) | null = null;

	// Mobile editor header helpers
	let noteRelativePath = $derived($activeNotePath && $appConfig?.active_vault ? $activeNotePath.replace($appConfig.active_vault + '/', '') : '');
	let isQuickAccess = $derived(noteRelativePath ? $quickAccessPaths.includes(noteRelativePath) : false);
	let backupInterval: ReturnType<typeof setInterval> | null = null;
	let syncInterval: ReturnType<typeof setInterval> | null = null;
	let unlistenSync: Array<() => void> = [];
	let unsubDirty: (() => void) | null = null;
	let onChangeSyncTimer: ReturnType<typeof setTimeout> | null = null;
	let prevDirty = false;



	function parseFrequencyMs(freq: string): number {
		switch (freq) {
			case '6h': return 6 * 60 * 60 * 1000;
			case '12h': return 12 * 60 * 60 * 1000;
			case '7d': return 7 * 24 * 60 * 60 * 1000;
			case '24h': default: return 24 * 60 * 60 * 1000;
		}
	}

	async function checkScheduledBackup() {
		const config = get(appConfig);
		if (!config?.backup_enabled) return;
		const interval = parseFrequencyMs(config.backup_frequency);
		const last = config.last_backup_time ? new Date(config.last_backup_time).getTime() : 0;
		if (Date.now() - last >= interval) {
			const unlisten = await listen('backup-done', (event: any) => {
				if (event.payload?.success) {
					const cur = get(appConfig);
					if (cur) appConfig.set({ ...cur, last_backup_time: new Date().toISOString() });
				}
				unlisten();
			});
			try { await createBackup(); } catch (_) { unlisten(); }
		}
	}

	function syncConfigured(): boolean {
		return activeVaultConfig(get(appConfig))?.sync_provider === 'webdav';
	}

	// Auto-sync on a timer (only when configured and an interval is set).
	async function checkScheduledSync() {
		const vc = activeVaultConfig(get(appConfig));
		if (vc?.sync_provider !== 'webdav') return;
		const mins = vc.sync_interval_minutes ?? 0;
		if (!mins || get(syncState).running) return;
		const last = vc.last_sync_time ? new Date(vc.last_sync_time).getTime() : 0;
		if (Date.now() - last >= mins * 60 * 1000) {
			try { await syncNow(); } catch (_) {}
		}
	}

	export async function triggerSyncNow() {
		if (!syncConfigured() || get(syncState).running) return;
		try { await syncNow(); } catch (_) {}
	}

	// Track note navigation in history stack
	$effect(() => {
		const path = $activeNotePath;
		if (path) navHistory.push(path);
	});

	function navigateHistory(direction: -1 | 1) {
		const path = navHistory.go(direction);
		if (!path) return;
		readNote(path).then((content) => {
			editor?.flushSave();
			$activeNote = content;
			$activeNotePath = path;
			$editorDirty = false;
			editor?.loadNote(path, content.content);
		}).catch(() => {});
	}

	async function handleOpenFile(filePath: string) {
		if (!filePath || !filePath.endsWith('.md')) return;
		const config = get(appConfig);
		const vaultRoot = config?.active_vault;
		const isExternal = !vaultRoot || !filePath.startsWith(vaultRoot + '/');

		// External .md → viewer mode (skipped on Android)
		if (isExternal) {
			if (isAndroid) return;
			try {
				const content = await readNote(filePath);
				editor?.flushSave();
				$viewerNote = { path: filePath, content: content.content };
				$activeNote = content;
				$activeNotePath = filePath;
				$editorDirty = false;
				$readOnly = true;
				$focusMode = true;
				editor?.loadNote(filePath, content.content);
			} catch (e) {
				console.error('Failed to open external file:', e);
			}
			return;
		}

		// Vault file: normal flow
		try {
			const content = await readNote(filePath);
			editor?.flushSave();
			$viewerNote = null;
			$activeNote = content;
			$activeNotePath = filePath;
			$editorDirty = false;
			editor?.loadNote(filePath, content.content);
		} catch (e) {
			console.error('Failed to open file:', e);
		}
	}

	const persistState = debounce(async () => {
		const state: VaultState = {
			last_open_note: $activeNotePath,
			sidebar_width: $sidebarWidth,
			notelist_width: $notelistWidth,
			outline_width: $outlineWidth,
			sidebar_collapsed: $sidebarCollapsed,
			notelist_collapsed: $notelistCollapsed,
			collapsed_notebooks: $collapsedNotebooks,
			notebook_sort_mode: $notebookSortMode,
			notebook_order: $notebookOrder,
			note_order: $noteOrder,
			sort_mode: $sortMode,
			group_notes_by_date: $groupNotesByDate,
			last_view_mode: $viewMode,
			last_notebook: $activeNotebook?.relative_path ?? null,
			last_tag: $activeTag,
			tasks_layout: $tasksLayout,
			tasks_hide_completed: $tasksHideCompleted,
			tasks_only_flagged: $tasksOnlyFlagged,
			tasks_sort: $tasksSort
		};
		try {
			await saveVaultState(state);
		} catch (_) {}
	}, 1000);

	function handleSidebarResize(delta: number) {
		$sidebarWidth = Math.max(160, Math.min(400, $sidebarWidth + delta));
		persistState();
	}

	function handleNotelistResize(delta: number) {
		$notelistWidth = Math.max(200, Math.min(500, $notelistWidth + delta));
		persistState();
	}

	function revealNoteList() {
		$notelistCollapsed = false;
		tick().then(() => noteList?.refresh());
	}

	function toggleNoteList() {
		const nextCollapsed = !$notelistCollapsed;
		$notelistCollapsed = nextCollapsed;
		if (!nextCollapsed) {
			tick().then(() => noteList?.refresh());
		}
	}

	// Tasks view: the editor pane shows a placeholder until a task is opened from the list.
	let taskNoteOpened = $state(false);

	function handleNoteSelected(path: string, content: string) {
		// Selecting a real vault note exits viewer mode
		$viewerNote = null;
		taskNoteOpened = true;
		editor?.loadNote(path, content);
		if (isMobile) $mobileView = 'editor';
	}

	function handleViewChanged() {
		taskNoteOpened = false;
		// Picking a notebook/tag/Tasks/etc. in the sidebar is a request to browse that view's
		// notes - so if the notes list was hidden, bring it back rather than navigating blindly
		// into a panel the user can't see. revealNoteList() re-mounts the list and refreshes it.
		if ($notelistCollapsed) {
			revealNoteList();
		} else {
			noteList?.refresh();
		}
		if (isMobile) $mobileView = 'notelist';
	}

	async function createAndFocusNote() {
		await noteList?.handleCreateNote();
		editor?.focusTitle();
		if (isMobile) $mobileView = 'editor';
	}

	// Toggle a task done from the Tasks view. If the task's note is open in the editor,
	// flush first then reload after the file edit (so we never clobber unsaved edits).
	async function toggleTask(task: TaskItem) {
		const done = !task.completed;
		const isActive = task.note_path === $activeNotePath;
		if (isActive) await editor?.forceSave();
		try {
			await setTaskDone(task.note_path, task.line, task.raw_line, done);
		} catch (e) {
			console.error('Failed to toggle task:', e);
		}
		if (isActive) {
			try {
				const content = await readNote(task.note_path);
				$activeNote = content;
				$editorDirty = false;
				editor?.loadNote(task.note_path, content.content);
			} catch (_) {}
		}
	}

	async function changeTaskPriority(task: TaskItem, priority: string | null) {
		const isActive = task.note_path === $activeNotePath;
		if (isActive) await editor?.forceSave();
		try {
			await setTaskPriority(task.note_path, task.line, task.raw_line, priority);
		} catch (e) {
			console.error('Failed to set task priority:', e);
		}
		if (isActive) {
			try {
				const content = await readNote(task.note_path);
				$activeNote = content;
				$editorDirty = false;
				editor?.loadNote(task.note_path, content.content);
			} catch (_) {}
		}
	}

	async function changeTaskDue(task: TaskItem, due: string | null) {
		const isActive = task.note_path === $activeNotePath;
		if (isActive) await editor?.forceSave();
		try {
			await setTaskDue(task.note_path, task.line, task.raw_line, due);
		} catch (e) {
			console.error('Failed to set task due date:', e);
		}
		if (isActive) {
			try {
				const content = await readNote(task.note_path);
				$activeNote = content;
				$editorDirty = false;
				editor?.loadNote(task.note_path, content.content);
			} catch (_) {}
		}
	}

	async function handleDailyNote() {
		try {
			const entry = await createDailyNote();
			const content = await readNote(entry.path);
			editor?.flushSave();
			$activeNote = content;
			$activeNotePath = entry.path;
			$editorDirty = false;
			editor?.loadNote(entry.path, content.content);
			$viewMode = 'daily';
			noteList?.refresh(true);
			sidebar?.refresh();
			if (isMobile) $mobileView = 'editor';
		} catch (e) {
			console.error('Failed to create/open daily note:', e);
		}
	}

	// Android back gesture / hardware back button support
	// We maintain a simple counter of how many views deep we are.
	// sidebar=0, notelist=1, editor=2. Each forward nav pushes, back pops.
	let historyDepth = 0;
	let navFromPopstate = false;

	if (get(platformIsMobile)) {
		history.replaceState({ mobileView: 'sidebar', depth: 0 }, '');

		$effect(() => {
			const view = $mobileView;
			if (navFromPopstate) {
				navFromPopstate = false;
				return;
			}
			const targetDepth = view === 'sidebar' ? 0 : view === 'notelist' ? 1 : 2;
			if (targetDepth > historyDepth) {
				// Forward navigation - push entries for each level skipped
				for (let d = historyDepth + 1; d <= targetDepth; d++) {
					const v = d === 1 ? 'notelist' : 'editor';
					history.pushState({ mobileView: v, depth: d }, '');
				}
				historyDepth = targetDepth;
			} else if (targetDepth < historyDepth) {
				const steps = historyDepth - targetDepth;
				historyDepth = targetDepth;
				navFromPopstate = true; // suppress the popstate that history.go triggers
				history.go(-steps);
			}
		});

		window.addEventListener('popstate', (e) => {
			if (navFromPopstate) {
				navFromPopstate = false;
				return;
			}
			// If a modal is open, close it instead of navigating
			if ($showSettings || $showInfo || $showSearch || $showCommandPalette) {
				$showSettings = false;
				$showInfo = false;
				$showSearch = false;
				$showCommandPalette = false;
				// Re-push the current state so the next back still works
				history.pushState({ mobileView: $mobileView, depth: historyDepth }, '');
				return;
			}
			const state = e.state;
			const targetDepth = state?.depth ?? 0;
			historyDepth = targetDepth;
			navFromPopstate = true;
			if (targetDepth === 0) $mobileView = 'sidebar';
			else if (targetDepth === 1) $mobileView = 'notelist';
			else $mobileView = 'editor';
		});
	}

	function mobileBack() {
		if ($mobileView === 'editor') $mobileView = 'notelist';
		else if ($mobileView === 'notelist') $mobileView = 'sidebar';
	}

	function handleMouseDown(e: MouseEvent) {
		if (e.button === 3) { e.preventDefault(); navigateHistory(-1); }
		if (e.button === 4) { e.preventDefault(); navigateHistory(1); }
	}

	function handleKeydown(e: KeyboardEvent) {
		const mod = e.ctrlKey || e.metaKey;
		const code = e.code;

		// Mod+K (insert link) is an editor formatting action and is not user-customizable.
		if (mod && code === 'KeyK' && !e.shiftKey && $activeNotePath && !$sourceMode) {
			e.preventDefault();
			editor?.addLinkFromToolbar();
		}

		// Ctrl/Cmd+Shift+Delete: move the open note to the trash.
		if (mod && e.shiftKey && code === 'Delete' && $activeNotePath) {
			e.preventDefault();
			noteList?.trashActiveNote();
			return;
		}

		const action = matchAction(e, $keybindings);
		if (action) {
			e.preventDefault();
			switch (action) {
				case 'nav-back':
					navigateHistory(-1);
					return;
				case 'nav-forward':
					navigateHistory(1);
					return;
				case 'new-note':
					createAndFocusNote();
					return;
				case 'toggle-theme': {
					const isDark = $theme === 'dark' || ($theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
					const next = isDark ? 'light' : 'dark';
					$theme = next;
					setTheme(next);
					return;
				}
				case 'search-vault':
					$showSearch = true;
					return;
				case 'find-in-note':
					if ($activeNotePath) {
						editor?.openNoteSearch();
					} else {
						$showSearch = true;
					}
					return;
				case 'quick-open':
					$showCommandPalette = true;
					return;
				case 'save':
					editor?.forceSave();
					return;
				case 'toggle-source':
					$sourceMode = !$sourceMode;
					return;
				case 'open-new-window':
					if ($activeNotePath && $activeNote) {
						openNoteWindow($activeNotePath, $activeNote.meta.title);
					}
					return;
				case 'toggle-sidebar':
					$sidebarCollapsed = !$sidebarCollapsed;
					return;
				case 'toggle-notelist':
					toggleNoteList();
					return;
				case 'toggle-focus':
					$focusMode = !$focusMode;
					return;
				case 'toggle-readonly':
					if ($viewerNote) return; // viewer mode is always read-only
					$readOnly = !$readOnly;
					return;
				case 'fullscreen':
					appWindow.isFullscreen().then(fs => appWindow.setFullscreen(!fs));
					return;
			}
		}

		if (e.key === 'Escape') {
			if ($showSettings) $showSettings = false;
			else if ($showInfo) $showInfo = false;
			else if ($focusMode) $focusMode = false;
			else if ($showSearch) $showSearch = false;
			else if ($showCommandPalette) $showCommandPalette = false;
		}
	}

	$effect(() => {
		applyTheme($theme, $customThemes);
	});

	$effect(() => {
		$collapsedNotebooks;
		persistState();
	});

	$effect(() => {
		$activeNotePath;
		persistState();
	});

	$effect(() => {
		$sidebarCollapsed;
		$notelistCollapsed;
		$outlineWidth;
		persistState();
	});

	$effect(() => {
		$notebookSortMode;
		$notebookOrder;
		persistState();
	});

	$effect(() => {
		$noteOrder;
		persistState();
	});

	$effect(() => {
		$sortMode;
		$groupNotesByDate;
		$tasksLayout;
		$tasksHideCompleted;
		$tasksOnlyFlagged;
		$tasksSort;
		persistState();
	});

	$effect(() => {
		$viewMode;
		$activeNotebook;
		$activeTag;
		persistState();
	});

	onMount(async () => {
		let lastNotePath: string | null = null;
		let lastViewMode = '';
		let lastNotebook: string | null = null;
		let lastTag: string | null = null;
		try {
			const state = await loadVaultState();
			$sidebarWidth = state.sidebar_width;
			$notelistWidth = state.notelist_width;
			if (typeof state.outline_width === 'number') $outlineWidth = state.outline_width;
			$sidebarCollapsed = state.sidebar_collapsed;
			$notelistCollapsed = state.notelist_collapsed ?? false;
			$collapsedNotebooks = state.collapsed_notebooks ?? [];
			$notebookSortMode = state.notebook_sort_mode === 'manual' ? 'manual' : 'alphabetical';
			$notebookOrder = state.notebook_order ?? {};
			$noteOrder = state.note_order ?? {};
			if (state.sort_mode === 'created' || state.sort_mode === 'title' || state.sort_mode === 'modified' || state.sort_mode === 'custom') $sortMode = state.sort_mode;
			if (typeof state.group_notes_by_date === 'boolean') $groupNotesByDate = state.group_notes_by_date;
			if (state.tasks_layout === 'calendar' || state.tasks_layout === 'list') $tasksLayout = state.tasks_layout;
			if (typeof state.tasks_hide_completed === 'boolean') $tasksHideCompleted = state.tasks_hide_completed;
			if (typeof state.tasks_only_flagged === 'boolean') $tasksOnlyFlagged = state.tasks_only_flagged;
			if (state.tasks_sort === 'due' || state.tasks_sort === 'priority' || state.tasks_sort === 'note') $tasksSort = state.tasks_sort;
			lastNotePath = state.last_open_note ?? null;
			lastViewMode = state.last_view_mode ?? '';
			lastNotebook = state.last_notebook ?? null;
			lastTag = state.last_tag ?? null;
		} catch (_) {}

		// On mobile, prefetch last-opened note so first tap is instant
		let prefetchPromise: Promise<any> | null = null;
		if (isMobile && lastNotePath) {
			prefetchPromise = readNote(lastNotePath).catch(() => null);
		}

		applyTheme($theme, $customThemes);

		window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
			if ($theme === 'system') applyTheme('system', $customThemes);
		});

		// Run sidebar and note list refresh in parallel
		await Promise.all([sidebar?.refresh(), noteList?.refresh()]);

		// Auto-cleanup orphaned attachments in the background
		setTimeout(async () => {
			if (get(editorDirty)) return; // skip if user is actively editing
			try {
				const orphans = await findOrphanedAttachments();
				if (orphans.length > 0) {
					if (get(editorDirty)) return; // re-check after async scan
					const moved = await trashOrphanedAttachments(orphans.map((o) => o.name));
					if (moved > 0) {
						const toast = document.createElement('div');
						toast.style.cssText = 'position:fixed;bottom:24px;left:50%;transform:translateX(-50%);background:var(--bg-secondary);color:var(--text-primary);padding:10px 18px;border-radius:8px;box-shadow:0 4px 12px rgba(0,0,0,.15);border:1px solid var(--border-color);font-size:13px;z-index:10000;opacity:0;transition:opacity .3s;pointer-events:none';
						toast.textContent = `Moved ${moved} orphaned attachment${moved > 1 ? 's' : ''} to trash`;
						document.body.appendChild(toast);
						requestAnimationFrame(() => (toast.style.opacity = '1'));
						setTimeout(() => { toast.style.opacity = '0'; setTimeout(() => toast.remove(), 300); }, 4000);
					}
				}
			} catch (_) {}
		}, 3000);

		// Restore the last session (view + open note) if enabled.
		if ($appConfig?.restore_last_session) {
			const vault = $appConfig?.active_vault;
			if (lastViewMode === 'notebook' && lastNotebook === '' && vault) {
				$viewMode = 'notebook';
				$activeNotebook = { name: 'Unfiled Notes', path: vault, relative_path: '', children: [], note_count: $rootNoteCount };
				$activeTag = null;
				await noteList?.refresh();
			} else if (lastViewMode === 'notebook' && lastNotebook) {
				const nb = findNotebookByPath($notebooks, lastNotebook);
				if (nb) { $viewMode = 'notebook'; $activeNotebook = nb; $activeTag = null; await noteList?.refresh(); }
			} else if (lastViewMode === 'tag' && lastTag) {
				$viewMode = 'tag'; $activeTag = lastTag; $activeNotebook = null; await noteList?.refresh();
			} else if (lastViewMode === 'quickaccess') {
				$viewMode = 'quickaccess'; $activeNotebook = null; $activeTag = null; await noteList?.refresh();
			}
			// Open the last note on desktop (mobile reopens it via its own prefetch path below).
			if (!isMobile && lastNotePath) {
				try {
					const content = await readNote(lastNotePath);
					$activeNote = content;
					$activeNotePath = lastNotePath;
					$editorDirty = false;
					editor?.loadNote(lastNotePath, content.content);
				} catch (_) {}
			}
		}

		// On mobile, derive tags from the scanned notes (avoids a separate full-scan Rust call)
		if (isMobile) {
			const tagMap = new Map<string, number>();
			for (const note of $notes) {
				for (const tag of note.meta.tags) {
					tagMap.set(tag, (tagMap.get(tag) ?? 0) + 1);
				}
			}
			$tags = Array.from(tagMap.entries()).sort((a, b) => a[0].localeCompare(b[0]));
		}

		// On mobile, auto-load the last-opened note so it's ready when the user taps it
		if (isMobile && prefetchPromise) {
			prefetchPromise.then((noteContent) => {
				if (noteContent && lastNotePath && !$activeNotePath) {
					$activeNote = noteContent;
					$activeNotePath = lastNotePath;
					$editorDirty = false;
					editor?.loadNote(lastNotePath, noteContent.content);
				}
			});
		}

		if (isMobile) {
			// Mobile: debounce file-changed heavily and skip when actively editing
			// (our own saves trigger the watcher, causing expensive refreshes on FUSE)
			const debouncedRefresh = debounce(async () => {
				if (get(editorDirty)) return; // user is still typing, skip
				await Promise.all([sidebar?.refresh(), noteList?.refresh(true)]);
				// Re-derive tags from refreshed notes
				const tagMap = new Map<string, number>();
				for (const note of get(notes)) {
					for (const tag of note.meta.tags) {
						tagMap.set(tag, (tagMap.get(tag) ?? 0) + 1);
					}
				}
				tags.set(Array.from(tagMap.entries()).sort((a, b) => a[0].localeCompare(b[0])));
		}, 10000);
			unlistenFileChange = await listen<FileEvent>('file-changed', () => {
				debouncedRefresh();
			});
		} else {
			const debouncedDesktopRefresh = debounce(async () => {
				await Promise.all([sidebar?.refresh(), noteList?.refresh(true)]);
			}, 300);
			unlistenFileChange = await listen<FileEvent>('file-changed', () => {
				debouncedDesktopRefresh();
			});
		}

		// Listen for file open events (from single-instance or OS file associations)
		unlistenOpenFile = await listen<string>('open-file', async (event) => {
			await handleOpenFile(event.payload);
		});

		// Check for pending file from first-launch CLI args
		try {
			const pending = await getPendingOpenFile();
			if (pending) await handleOpenFile(pending);
		} catch (_) {}

		// Scheduled backup: check on startup and every 5 minutes
		checkScheduledBackup();
		backupInterval = setInterval(checkScheduledBackup, 5 * 60 * 1000);

		// ── WebDAV sync: global status + auto-sync triggers ──
		unlistenSync.push(await listen('sync-progress', () => syncState.set({ running: true, error: null })));
		unlistenSync.push(await listen('sync-done', (event: any) => {
			syncState.set({ running: false, error: null });
			const cur = get(appConfig);
			const ts = event.payload?.last_sync_time;
			if (cur && ts) appConfig.set({ ...cur, vaults: cur.vaults.map((v) => v.path === cur.active_vault ? { ...v, last_sync_time: ts } : v) });
		}));
		unlistenSync.push(await listen('sync-error', (event: any) => syncState.set({ running: false, error: event.payload?.error ?? 'Sync failed' })));

		// Sync when the vault opens (if enabled)
		if (syncConfigured() && activeVaultConfig(get(appConfig))?.sync_on_open) {
			try { await syncNow(); } catch (_) {}
		}

		// Auto-sync interval: check on startup and every minute
		checkScheduledSync();
		syncInterval = setInterval(checkScheduledSync, 60 * 1000);

		// Auto-sync on note change: a save flips editorDirty true -> false. Debounce a sync.
		unsubDirty = editorDirty.subscribe((d) => {
			const vc = activeVaultConfig(get(appConfig));
			if (prevDirty && !d && vc?.sync_provider === 'webdav' && vc?.sync_on_change) {
				if (onChangeSyncTimer) clearTimeout(onChangeSyncTimer);
				onChangeSyncTimer = setTimeout(() => { if (!get(syncState).running) syncNow().catch(() => {}); }, 15000);
			}
			prevDirty = d;
		});
	});

	onDestroy(() => {
		unlistenFileChange?.();
		unlistenOpenFile?.();
		if (backupInterval) clearInterval(backupInterval);
		if (syncInterval) clearInterval(syncInterval);
		if (onChangeSyncTimer) clearTimeout(onChangeSyncTimer);
		unsubDirty?.();
		unlistenSync.forEach((u) => u());
	});
</script>

<svelte:window onkeydown={handleKeydown} onmousedown={isMobile ? undefined : handleMouseDown} />

{#if isMobile}
	<!-- ═══ MOBILE LAYOUT ═══ -->
	<div class="mobile-shell">
		<!-- Mobile Header -->
		<div class="mobile-header">
			{#if $mobileView !== 'sidebar'}
				<button class="mobile-header-btn" onclick={mobileBack}>
					<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M15 18l-6-6 6-6"/>
					</svg>
				</button>
			{:else}
				<div class="mobile-header-brand">
					<svg width="22" height="22" viewBox="0 0 48 48" fill="none">
						<rect width="48" height="48" rx="12" fill="var(--accent)" />
						<circle cx="16" cy="16" r="3.5" fill="white" opacity="0.9" />
						<circle cx="32" cy="16" r="3.5" fill="white" opacity="0.9" />
						<circle cx="16" cy="32" r="3.5" fill="white" opacity="0.9" />
						<circle cx="32" cy="32" r="3.5" fill="white" opacity="0.9" />
					</svg>
				</div>
			{/if}
			{#if $mobileView !== 'editor'}
				<span class="mobile-header-title">
					{#if $mobileView === 'sidebar'}
						HelixNotes
					{:else}
						{#if $viewMode === 'notebook'}{$activeNotebook?.name ?? 'Notebook'}{:else if $viewMode === 'tag'}#{$activeTag}{:else if $viewMode === 'quickaccess'}Quick Access{:else if $viewMode === 'daily'}Daily Notes{:else if $viewMode === 'tasks'}Tasks{:else if $viewMode === 'trash'}Trash{:else}All Notes{/if}
					{/if}
				</span>
				{#if $globalUpdateAvailable && $mobileView === 'sidebar'}
					<button class="mobile-update-badge" onclick={() => { $settingsTab = 'updates'; $showSettings = true; }}>
						v{$globalUpdateAvailable.version}
					</button>
				{/if}
			{/if}
			<div class="mobile-header-actions">
				{#if $mobileView === 'editor'}
					<button class="mobile-header-btn" class:active={$readOnly} onclick={() => ($readOnly = !$readOnly)} title={$readOnly ? 'Edit' : 'View'}>
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							{#if $readOnly}
								<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
								<circle cx="12" cy="12" r="3" />
							{:else}
								<path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
								<path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
							{/if}
						</svg>
					</button>
					<button class="mobile-header-btn" onclick={() => editor?.openNoteSearch()} title="Find in note">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
						</svg>
					</button>
					<button class="mobile-header-btn" class:active={$activeNote?.meta.pinned} onclick={() => { if ($activeNote) { $activeNote.meta.pinned = !$activeNote.meta.pinned; $editorDirty = true; } }} title="Pin">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<path d="M12 17v5"/><path d="M9 2h6l-1 7h4l-2 4H8l-2-4h4L9 2z"/>
						</svg>
					</button>
					<button class="mobile-header-btn" class:active={isQuickAccess} onclick={async () => {
						if (!noteRelativePath) return;
						try {
							if (isQuickAccess) { await removeQuickAccess(noteRelativePath); } else { await addQuickAccess(noteRelativePath); }
							$quickAccessPaths = (await getQuickAccess()).map(n => n.relative_path);
						} catch (e) { console.error('Quick access toggle failed:', e); }
					}} title="Quick Access">
						<svg width="18" height="18" viewBox="0 0 24 24" fill={isQuickAccess ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
						</svg>
					</button>
					<button class="mobile-header-btn" onclick={() => editor?.toggleOutlinePanel()} title="Outline">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<line x1="4" y1="6" x2="20" y2="6"/><line x1="8" y1="12" x2="20" y2="12"/><line x1="8" y1="18" x2="20" y2="18"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/>
						</svg>
					</button>
					<button class="mobile-header-btn" onclick={() => editor?.toggleHistoryPanel()} title="History">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
						</svg>
					</button>
					{#if $appConfig?.enable_wiki_links}
					<button class="mobile-header-btn" onclick={() => editor?.toggleGraphView()} title="Graph View">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/>
							<line x1="8.5" y1="7.5" x2="15.5" y2="16.5"/><line x1="15.5" y1="7.5" x2="8.5" y2="16.5"/>
						</svg>
					</button>
					{/if}
					{#if $appConfig?.ai_provider}
					<button class="mobile-header-btn" onclick={() => editor?.triggerAiMenu()} title="AI Actions">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<path d="M12 8V4l-2-2"/><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M9 13v2"/><path d="M15 13v2"/>
						</svg>
					</button>
					{/if}
					{#if activeVaultConfig($appConfig)?.sync_provider === 'webdav'}
					<button class="mobile-header-btn" class:active={$syncState.running} onclick={triggerSyncNow} disabled={$syncState.running} title={$syncState.error ? `Sync error: ${$syncState.error}` : ($syncState.running ? 'Syncing...' : 'Sync now')}>
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class:sync-spin={$syncState.running}>
							<path d="M21 2v6h-6"/><path d="M3 12a9 9 0 0115-6.7L21 8"/><path d="M3 22v-6h6"/><path d="M21 12a9 9 0 01-15 6.7L3 16"/>
						</svg>
					</button>
					{/if}
					<button class="mobile-header-btn" class:active={$sourceMode} onclick={() => ($sourceMode = !$sourceMode)} title={$sourceMode ? 'Rich Editor' : 'Source Mode'}>
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" />
						</svg>
					</button>
				{:else}
					<button class="mobile-header-btn" onclick={() => ($showSearch = true)} title="Search">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="11" cy="11" r="8" />
							<line x1="21" y1="21" x2="16.65" y2="16.65" />
						</svg>
					</button>
						<button class="mobile-header-btn" onclick={handleDailyNote} title="Daily Note">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
							<line x1="16" y1="2" x2="16" y2="6" />
							<line x1="8" y1="2" x2="8" y2="6" />
							<line x1="3" y1="10" x2="21" y2="10" />
						</svg>
					</button>
					<button class="mobile-header-btn" onclick={() => ($showInfo = true)} title="Info">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="12" cy="12" r="10" />
							<line x1="12" y1="16" x2="12" y2="12" />
							<line x1="12" y1="8" x2="12.01" y2="8" />
						</svg>
					</button>
					<button class="mobile-header-btn" onclick={() => ($showSettings = true)} title="Settings">
						<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<circle cx="12" cy="12" r="3" />
							<path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 01-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" />
						</svg>
					</button>
				{/if}
			</div>
		</div>

		<!-- Mobile Content -->
		<div class="mobile-content">
			<div class="mobile-panel" class:active={$mobileView === 'sidebar'}>
				<Sidebar bind:this={sidebar} onViewChanged={handleViewChanged} />
			</div>
			<div class="mobile-panel" class:active={$mobileView === 'notelist'}>
				<NoteList bind:this={noteList} onNoteSelected={handleNoteSelected} onBeforeNoteSwitch={() => editor?.flushSave()} onNoteMoved={() => sidebar?.refresh()} onNoteCreated={() => { editor?.focusTitle(); }} onToggleTask={toggleTask} onSetTaskPriority={changeTaskPriority} onSetTaskDue={changeTaskDue} />
			</div>
			<div class="mobile-panel" class:active={$mobileView === 'editor'}>
				<Editor bind:this={editor} />
			</div>
		</div>

	</div>
{:else}
	<!-- ═══ DESKTOP LAYOUT ═══ -->
	<div class="app-shell">
		{#if $focusMode}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="focus-topbar" class:macos={isMac} onmousedown={(e) => { if (!(e.target as HTMLElement).closest('button')) appWindow.startDragging(); }}>
				<span class="focus-title">{$activeNote?.meta.title || 'Untitled'}</span>
				<div class="focus-controls">
					<button class="focus-btn focus-active" onclick={() => ($focusMode = false)} title="Exit focus mode (Escape)">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
							<path d="M8 3v3a2 2 0 01-2 2H3m18 0h-3a2 2 0 01-2-2V3m0 18v-3a2 2 0 012-2h3M3 16h3a2 2 0 012 2v3"/>
						</svg>
					</button>
					<button class="focus-btn" class:focus-active={$readOnly} onclick={() => ($readOnly = !$readOnly)} title={$readOnly ? 'Switch to Edit Mode' : 'Switch to View Mode'}>
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
					{#if !isMac}
					<button class="focus-btn" onmousedown={(e) => e.stopPropagation()} onclick={() => appWindow.minimize()} title="Minimize">
						<svg width="10" height="10" viewBox="0 0 10 10"><line x1="1" y1="5" x2="9" y2="5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
					</button>
					<button class="focus-btn" onmousedown={(e) => e.stopPropagation()} onclick={() => appWindow.toggleMaximize()} title="Maximize">
						<svg width="10" height="10" viewBox="0 0 10 10"><rect x="1" y="1" width="8" height="8" rx="1" fill="none" stroke="currentColor" stroke-width="1.2"/></svg>
					</button>
					<button class="focus-btn focus-close" onmousedown={(e) => e.stopPropagation()} onclick={() => appWindow.close()} title="Close">
						<svg width="10" height="10" viewBox="0 0 10 10"><line x1="1.5" y1="1.5" x2="8.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><line x1="8.5" y1="1.5" x2="1.5" y2="8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
					</button>
					{/if}
				</div>
			</div>
		{:else}
			<TitleBar onNewNote={createAndFocusNote} onDailyNote={handleDailyNote} />
		{/if}
		<div class="app-layout">
			{#if !$focusMode}
				<div class="sidebar-panel" style="width: {$sidebarCollapsed ? 44 : $sidebarWidth}px">
					<Sidebar bind:this={sidebar} onViewChanged={handleViewChanged} />
				</div>

				{#if !$sidebarCollapsed}
					<ResizeHandle onResize={handleSidebarResize} />
				{/if}

				{#if !$notelistCollapsed}
					<div class="notelist-panel" style="width: {$notelistWidth}px">
						<NoteList bind:this={noteList} onNoteSelected={handleNoteSelected} onBeforeNoteSwitch={() => editor?.flushSave()} onNoteMoved={() => sidebar?.refresh()} onNoteCreated={() => { editor?.focusTitle(); }} onToggleTask={toggleTask} onSetTaskPriority={changeTaskPriority} onSetTaskDue={changeTaskDue} />
					</div>

					<ResizeHandle onResize={handleNotelistResize} />
				{:else}
					<div class="notelist-restore-panel">
						<button class="notelist-restore-btn" onclick={revealNoteList} title={`Show notes list (${isMac ? '⌘' : 'Ctrl'}+Shift+\\)`} aria-label="Show notes list">
							<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
								<line x1="8" y1="6" x2="20" y2="6" />
								<line x1="8" y1="12" x2="20" y2="12" />
								<line x1="8" y1="18" x2="20" y2="18" />
								<polyline points="5 8 3 12 5 16" />
							</svg>
						</button>
					</div>
				{/if}
			{/if}

			<div class="editor-panel">
				<Editor bind:this={editor} />
				{#if $viewMode === 'tasks' && !taskNoteOpened}
					<div class="tasks-editor-placeholder">
						<svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
							<path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/>
						</svg>
						<p>Select a task to open its note</p>
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<SearchPanel />
<CommandPalette onNavigate={handleViewChanged} />
<SettingsPanel />
<InfoPanel />

<style>
	/* ═══ DESKTOP STYLES ═══ */
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.app-layout {
		display: flex;
		flex: 1;
		overflow: hidden;
	}

	.sidebar-panel {
		flex-shrink: 0;
		height: 100%;
		overflow: hidden;
	}

	.notelist-panel {
		flex-shrink: 0;
		height: 100%;
		overflow: hidden;
	}

	.notelist-restore-panel {
		flex-shrink: 0;
		width: 36px;
		height: 100%;
		padding-top: 8px;
		display: flex;
		justify-content: center;
		background: var(--bg-primary);
		border-right: 1px solid var(--border-color);
	}

	.notelist-restore-btn {
		width: 24px;
		height: 24px;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--text-tertiary);
		cursor: pointer;
	}

	.notelist-restore-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.editor-panel {
		flex: 1;
		height: 100%;
		overflow: hidden;
		min-width: 300px;
		position: relative;
	}

	.tasks-editor-placeholder {
		position: absolute;
		inset: 0;
		z-index: 5;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		background: var(--bg-primary);
		color: var(--text-tertiary);
		font-size: 14px;
	}

	.focus-topbar {
		display: flex;
		align-items: center;
		height: 34px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-light);
		user-select: none;
		flex-shrink: 0;
		padding: 0 8px 0 16px;
	}

	.focus-title {
		flex: 1;
		font-size: 12px;
		color: var(--text-tertiary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		pointer-events: none;
	}

	.focus-controls {
		display: flex;
		align-items: center;
		gap: 2px;
	}

	.focus-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 26px;
		border: none;
		background: none;
		color: var(--text-tertiary);
		cursor: pointer;
		border-radius: 4px;
		transition: background 0.1s, color 0.1s;
	}

	.focus-btn.focus-active {
		color: var(--text-accent);
		background: var(--accent-light);
	}

	.focus-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.focus-close:hover {
		background: #e81123;
		color: white;
	}

	.focus-topbar.macos {
		padding-left: 78px;
	}

	/* ═══ MOBILE STYLES ═══ */
	.mobile-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
		background: var(--bg-primary);
	}

	.mobile-header {
		display: flex;
		align-items: center;
		height: 52px;
		padding: 0 8px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-color);
		flex-shrink: 0;
		gap: 4px;
	}

	.mobile-header-brand {
		display: flex;
		align-items: center;
		padding: 0 8px;
	}

	.mobile-header-title {
		flex: 1;
		font-size: 17px;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		padding: 0 4px;
	}

	.mobile-update-badge {
		background: var(--accent);
		color: white;
		border: none;
		border-radius: 10px;
		padding: 2px 8px;
		font-size: 11px;
		font-weight: 600;
		font-family: inherit;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.mobile-header-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		margin-left: auto;
	}

	.mobile-header-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 34px;
		height: 34px;
		border: none;
		background: none;
		color: var(--text-secondary);
		border-radius: 8px;
		cursor: pointer;
	}

	.mobile-header-btn svg {
		width: 16px;
		height: 16px;
	}

	.mobile-header-btn:active {
		background: var(--bg-hover);
	}

	.mobile-header-btn.active {
		color: var(--accent);
	}

	.mobile-content {
		flex: 1;
		overflow: hidden;
		position: relative;
	}

	.mobile-panel {
		position: absolute;
		inset: 0;
		overflow: hidden;
		visibility: hidden;
		pointer-events: none;
		display: flex;
		flex-direction: column;
	}

	.mobile-panel.active {
		visibility: visible;
		pointer-events: auto;
	}

</style>
