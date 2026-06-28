<script lang="ts">
	import { onDestroy, tick, untrack } from 'svelte';
	import { get } from 'svelte/store';
	import { Editor } from '@tiptap/core';
	import StarterKit from '@tiptap/starter-kit';
	import Placeholder from '@tiptap/extension-placeholder';
	import TaskList from '@tiptap/extension-task-list';
	import TaskItem from '@tiptap/extension-task-item';
	import { Table } from '@tiptap/extension-table';
	import { TableRow } from '@tiptap/extension-table-row';
	import { TableCell } from '@tiptap/extension-table-cell';
	import { TableHeader } from '@tiptap/extension-table-header';
	import Link from '@tiptap/extension-link';
	import Image from '@tiptap/extension-image';
	import Highlight from '@tiptap/extension-highlight';
	import Typography from '@tiptap/extension-typography';
	import Underline from '@tiptap/extension-underline';
	import Subscript from '@tiptap/extension-subscript';
	import Superscript from '@tiptap/extension-superscript';
	import { Color } from '@tiptap/extension-color';
	import { TextStyle } from '@tiptap/extension-text-style';
	import { CodeBlockLowlight } from '@tiptap/extension-code-block-lowlight';
	import { Details, DetailsSummary, DetailsContent } from '@tiptap/extension-details';
	import TextAlign from '@tiptap/extension-text-align';
	import { common, createLowlight } from 'lowlight';
	import powershell from 'highlight.js/lib/languages/powershell';
	import MarkdownIt from 'markdown-it';
	import markdownItMark from 'markdown-it-mark';
	import markdownItSup from 'markdown-it-sup';
	import markdownItSub from 'markdown-it-sub';
	import katex from 'katex';
	import 'katex/dist/katex.min.css';
	import { Extension, Node as TiptapNode, Mark as TiptapMark, mergeAttributes } from '@tiptap/core';
	import { Plugin, PluginKey, EditorState, Selection, TextSelection } from '@tiptap/pm/state';
	import { Decoration, DecorationSet } from '@tiptap/pm/view';
	import { DOMSerializer } from '@tiptap/pm/model';
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { readFile } from '@tauri-apps/plugin-fs';
	import { openFile, openUrl, copyFileTo, copyImageToClipboard as copyImageToClipboardCmd, writeBytesTo, copyPngToClipboard } from '$lib/api';
	import { save as saveDialog } from '@tauri-apps/plugin-dialog';
	import { activeNote, activeNotePath, appConfig, editorDirty, sourceMode, focusMode, readOnly, quickAccessPaths, notes, navHistory, canGoBack, canGoForward, viewerNote, notebooks, outlineWidth } from '$lib/stores/app';
	import { saveNote, saveImage, saveAttachment, readClipboardImage, addQuickAccess, removeQuickAccess, getQuickAccess, getNoteVersions, getNoteVersionContent, createVersion, aiAsk, getAllNoteTitles, readNote, renameNote } from '$lib/api';
	import type { VersionEntry, AiStreamEvent, NoteTitleEntry } from '$lib/types';
	import { listen } from '@tauri-apps/api/event';
	import { debounce } from '$lib/utils/debounce';
	import { encryptSecretText, decryptSecretText, readSecretTitle } from '$lib/utils/secrets';
	import { WrapSelectedText } from '$lib/editor/extensions/wrapSelectedText';
	import { calloutGroup, calloutIcon, calloutLabel, CALLOUT_MENU, transformCalloutBlockquotes, serializeCallout } from '$lib/editor/callouts';
	import { wrapTextareaSelection } from '$lib/editor/source/selectionPairs';
	import GraphView from './GraphView.svelte';
	import TagSuggestInput from './TagSuggestInput.svelte';
	import { isMobile, isAndroid } from '$lib/platform';
	import ResizeHandle from './ResizeHandle.svelte';

	const modKey = navigator.platform.startsWith('Mac') ? '⌘' : 'Ctrl';

	// Track virtual keyboard height on mobile via visualViewport
	let keyboardHeight = $state(0);
	if (isMobile && typeof window !== 'undefined' && window.visualViewport) {
		const vv = window.visualViewport;
		const update = () => { keyboardHeight = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop)); };
		vv.addEventListener('resize', update);
		vv.addEventListener('scroll', update);
	}

	let editorElement = $state<HTMLDivElement>(null!);
	let sourceElement = $state<HTMLTextAreaElement>(null!);
	const LARGE_DOC_CHARS = 100_000;
	let isLargeDoc = $state(false);
	let editor: Editor | null = null;
	let editorReady = $state(false);
	let sourceContent = $state('');
	let sourceHistory: Array<{ content: string; cursor: number }> = [];
	let sourceHistoryIndex = -1;
	let sourceHistoryTimer: ReturnType<typeof setTimeout> | null = null;
	let loadedPath = '';
	let pendingContent = $state<string | null>(null);
	let ignoreNextUpdate = false;
	let isLoadingNote = false;
	let fixingBlobsPromise: Promise<void> = Promise.resolve();
	let hasPendingBlobs = false;
	let lastSourceMode = $sourceMode;
	let linkContextMenu = $state<{ x: number; y: number; href: string; anchor: HTMLAnchorElement } | null>(null);
	let titleWasStripped = false;
	let strippedTitle = '';
	let strippedHeadingPrefix = '';

	let headingDropdown = $state(false);
	let colorDropdown = $state(false);
	let highlightDropdown = $state(false);
	let alignDropdown = $state(false);
	let insertDropdown = $state(false);

	function scrollEditorBodyToBottom(source: HTMLElement | null | undefined = editorElement) {
		const editorBody = source?.closest('.editor-body') as HTMLElement | null;
		if (!editorBody) return;
		editorBody.scrollTop = editorBody.scrollHeight;
		requestAnimationFrame(() => {
			editorBody.scrollTop = editorBody.scrollHeight;
		});
	}

	function handleSourceCtrlEnd(event: KeyboardEvent) {
		if (!event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.key !== 'End') return false;
		event.preventDefault();
		const ta = sourceElement;
		ta.focus({ preventScroll: true });
		ta.setSelectionRange(sourceContent.length, sourceContent.length);
		ta.scrollTop = ta.scrollHeight;
		requestAnimationFrame(() => {
			ta.scrollTop = ta.scrollHeight;
		});
		return true;
	}

	function handleSourceSelectionPair(event: KeyboardEvent) {
		if (event.ctrlKey || event.metaKey || event.altKey || event.key.length !== 1) return false;
		const ta = sourceElement;
		if (!ta) return false;
		const wrapped = wrapTextareaSelection(ta.value, ta.selectionStart, ta.selectionEnd, event.key);
		if (!wrapped) return false;

		event.preventDefault();
		pushSourceHistoryImmediate();
		sourceContent = wrapped.value;
		tick().then(() => {
			ta.setSelectionRange(wrapped.selectionStart, wrapped.selectionEnd);
			pushSourceHistoryImmediate();
		});
		$editorDirty = true;
		autoSave();
		return true;
	}

	function closeAllDropdowns() {
		headingDropdown = false;
		colorDropdown = false;
		highlightDropdown = false;
		alignDropdown = false;
		insertDropdown = false;
		tablePickerOpen = false;
	}

	let anyDropdownOpen = $derived(headingDropdown || colorDropdown || highlightDropdown || alignDropdown || insertDropdown || tablePickerOpen);
	let editorState = $state(0);
	let editorStateRaf = 0; // RAF handle for batching toolbar updates

	// AI
	let aiMenu = $state<{ x: number; y: number } | null>(null);
	let aiLoading = $state(false);
	let aiResult = $state<string | null>(null);
	let aiError = $state<string | null>(null);
	let aiSelectionFrom = $state(0);
	let aiSelectionTo = $state(0);
	let aiSelectedText = $state('');
	let aiCustomPrompt = $state('');
	let aiShowCustom = $state(false);
	let aiTranslateMenu = $state(false);
	let aiWholeNote = $state(false);
	let aiEmptyNote = $state(false);
	let aiOriginalMarkdown = $state('');
	let aiMediaPlaceholders = $state<Map<string, string>>(new Map());
	let aiStreamUnlisten: (() => void) | null = null;


	// Outline
	let showOutline = $state(false);
	interface OutlineHeading { level: number; text: string; pos: number; }
	let outlineHeadings = $state<OutlineHeading[]>([]);

	function updateOutline() {
		if (!editor) { outlineHeadings = []; return; }
		const headings: OutlineHeading[] = [];
		editor.state.doc.descendants((node, pos) => {
			if (node.type.name === 'heading') {
				headings.push({ level: node.attrs.level, text: node.textContent, pos });
			}
		});
		outlineHeadings = headings;
	}

	const scheduleOutline = debounce(updateOutline, 250);

	function handleOutlineResize(delta: number) {
		$outlineWidth = Math.max(160, Math.min(500, $outlineWidth - delta));
	}

	function scrollToHeading(pos: number) {
		if (!editor) return;
		editor.commands.setTextSelection(pos + 1);
		editor.commands.scrollIntoView();
		editor.view.focus();
	}

	// Version history
	let showHistory = $state(false);
	let showGraph = $state(false);
	let tagMenu = $state<{ x: number; y: number } | null>(null);
	let historyVersions = $state<VersionEntry[]>([]);
	let historyPreview = $state<string | null>(null);
	let historySelected = $state<VersionEntry | null>(null);
	let historyLoading = $state(false);

	// Info panel
	let showInfo = $state(false);
	let infoPanelEl = $state<HTMLElement | null>(null);
	let infoToggleBtnEl = $state<HTMLElement | null>(null);
	let wordCount = $state(0);
	let charCount = $state(0);

	// In-note search
	let noteSearchOpen = $state(false);
	let noteSearchQuery = $state('');
	let noteSearchIndex = $state(0);
	let noteSearchResults = $state<{from: number, to: number}[]>([]);
	let noteSearchInput = $state<HTMLInputElement>(null!);
	const noteSearchPluginKey = new PluginKey('noteSearch');

	// Slash commands
	let slashMenu = $state<{ x: number; y: number; query: string; from: number; to: number } | null>(null);
	let slashSelectedIndex = $state(0);
	let slashTablePicker = $state(false);
	let slashTableHover = $state({ rows: 0, cols: 0 });
	let slashColorPicker = $state(false);
	let slashColorHex = $state('#4b6abf');
	let slashColorInputEl = $state<HTMLInputElement | null>(null);
	const colorPresets = ['#ef4444', '#f97316', '#eab308', '#22c55e', '#06b6d4', '#3b82f6', '#6366f1', '#a855f7', '#ec4899', '#64748b', '#000000', '#ffffff'];

	interface SlashCommand {
		label: string;
		aliases: string[];
		icon: string;
		action: () => void;
	}

	function insertTimestamp(kind: 'date' | 'time' | 'datetime') {
		if (!editor) return;
		const now = new Date();
		const pad = (n: number) => String(n).padStart(2, '0');
		const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
		const time = `${pad(now.getHours())}:${pad(now.getMinutes())}`;
		const text = kind === 'date' ? date : kind === 'time' ? time : `${date} ${time}`;
		editor.chain().focus().insertContent(text).run();
	}

	function getSlashCommands(): SlashCommand[] {
		return [
			{ label: 'Heading 1', aliases: ['h1', 'heading1', 'title'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12h8M4 4v16M12 4v16M17 12l3-2v8"/></svg>', action: () => editor?.chain().focus().toggleHeading({ level: 1 }).run() },
			{ label: 'Heading 2', aliases: ['h2', 'heading2', 'subtitle'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12h8M4 4v16M12 4v16"/><path d="M21 18h-4c0-4 4-3 4-6 0-1.5-2-2.5-4-1"/></svg>', action: () => editor?.chain().focus().toggleHeading({ level: 2 }).run() },
			{ label: 'Heading 3', aliases: ['h3', 'heading3'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M4 12h8M4 4v16M12 4v16"/><path d="M17.5 10.5c1.7-1 3.5 0 3.5 1.5a2 2 0 01-2 2m2 0a2 2 0 01-2 2c-1.5 0-3.5 0-3.5-1.5"/></svg>', action: () => editor?.chain().focus().toggleHeading({ level: 3 }).run() },
			{ label: 'Bullet List', aliases: ['ul', 'unordered', 'bullets', 'list'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><circle cx="3" cy="6" r="1" fill="currentColor"/><circle cx="3" cy="12" r="1" fill="currentColor"/><circle cx="3" cy="18" r="1" fill="currentColor"/></svg>', action: () => editor?.chain().focus().toggleBulletList().run() },
			{ label: 'Numbered List', aliases: ['ol', 'ordered', 'number'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><text x="1" y="9" font-size="8" fill="currentColor" stroke="none">1</text><text x="1" y="15" font-size="8" fill="currentColor" stroke="none">2</text><text x="1" y="21" font-size="8" fill="currentColor" stroke="none">3</text></svg>', action: () => editor?.chain().focus().toggleOrderedList().run() },
			{ label: 'Task List', aliases: ['checklist', 'checkbox', 'todo', 'check'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="5" width="6" height="6" rx="1"/><path d="M5 8l1.5 1.5L9 7"/><line x1="13" y1="8" x2="21" y2="8"/><rect x="3" y="14" width="6" height="6" rx="1"/><line x1="13" y1="17" x2="21" y2="17"/></svg>', action: () => editor?.chain().focus().toggleTaskList().run() },
			{ label: 'Code Block', aliases: ['code', 'codeblock', 'pre', 'snippet'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>', action: () => editor?.chain().focus().toggleCodeBlock().run() },
			{ label: 'Secret', aliases: ['secret', 'encrypt', 'password', 'private'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="10" rx="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>', action: () => openSecretInsert() },
			{ label: 'Blockquote', aliases: ['quote', 'blockquote', 'citation'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 21c3 0 7-1 7-8V5c0-1.25-.756-2.017-2-2H4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2 1 0 1 0 1 1v1c0 1-1 2-2 2s-1 .008-1 1.031V20c0 1 0 1 1 1z"/><path d="M15 21c3 0 7-1 7-8V5c0-1.25-.757-2.017-2-2h-4c-1.25 0-2 .75-2 1.972V11c0 1.25.75 2 2 2h.75c0 2.25.25 4-2.75 4v3c0 1 0 1 1 1z"/></svg>', action: () => editor?.chain().focus().toggleBlockquote().run() },
			{ label: 'Collapsible Section', aliases: ['details', 'accordion', 'collapse', 'toggle', 'summary'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><polyline points="10 8 14 12 10 16"/></svg>', action: () => insertDetails() },
			{ label: 'Callout', aliases: ['callout', 'admonition', 'note', 'info', 'tip', 'warning', 'caution', 'danger', 'success', 'question', 'quote', 'aside', 'box'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2"/><line x1="7" y1="5" x2="7" y2="19"/></svg>', action: () => insertCallout('note') },
			{ label: 'Table', aliases: ['table', 'grid'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><line x1="3" y1="15" x2="21" y2="15"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg>', action: () => { slashTablePicker = true; slashTableHover = { rows: 0, cols: 0 }; } },
			{ label: 'Horizontal Rule', aliases: ['hr', 'divider', 'line', 'separator', 'rule'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="2" y1="12" x2="22" y2="12"/></svg>', action: () => editor?.chain().focus().setHorizontalRule().run() },
			{ label: 'Page Break', aliases: ['pagebreak', 'page', 'break', 'newpage', 'print'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="2" y1="9" x2="22" y2="9" stroke-dasharray="4 2"/><line x1="2" y1="15" x2="22" y2="15" stroke-dasharray="4 2"/><path d="M6 5v4M18 5v4M6 15v4M18 15v4"/></svg>', action: () => editor?.chain().focus().insertContent({ type: 'pageBreak' }).run() },
			{ label: 'Math Block', aliases: ['math', 'latex', 'equation', 'formula', 'tex', 'katex'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h6l4 14h6"/><path d="M7 19l10-14"/></svg>', action: () => openMathInsert('block') },
			{ label: 'Math Inline', aliases: ['mathinline', 'inline-math', 'imath', 'inlinemath'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 8h2l3 8h2"/><path d="M8 12l8-4"/></svg>', action: () => openMathInsert('inline') },
			{ label: 'Date', aliases: ['date', 'today', 'day'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>', action: () => insertTimestamp('date') },
			{ label: 'Time', aliases: ['time', 'clock'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>', action: () => insertTimestamp('time') },
			{ label: 'Date & Time', aliases: ['datetime', 'now', 'timestamp', 'stamp'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h6"/><line x1="3" y1="10" x2="21" y2="10"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="16" y1="2" x2="16" y2="6"/><circle cx="18" cy="17" r="4"/><path d="M18 15.5v1.5l1 1"/></svg>', action: () => insertTimestamp('datetime') },
			{ label: 'Color', aliases: ['color', 'colour', 'hex', 'rgb', 'swatch', 'palette'], icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h1.996c3.051 0 5.555-2.503 5.555-5.554C21.965 6.012 17.461 2 12 2z"/></svg>', action: () => { slashColorPicker = true; } },
		];
	}

	let slashFiltered = $derived.by(() => {
		const commands = getSlashCommands();
		if (!slashMenu) return commands;
		const q = slashMenu.query.toLowerCase();
		if (!q) return commands;
		return commands.filter(cmd =>
			cmd.label.toLowerCase().includes(q) ||
			cmd.aliases.some(a => a.includes(q))
		);
	});
	let titleInput = $state<HTMLInputElement>(null!);
	let linkModal = $state(false);
	let linkModalUrl = $state('');
	let linkModalInput = $state<HTMLInputElement>(null!);
	let linkSelectionFrom = 0;
	let linkSelectionTo = 0;
	let linkModalDisplayText = '';
	let linkSuggestIndex = $state(0);
	let linkSuggestTitles = $state<NoteTitleEntry[]>([]);
	let linkSuggestFiltered = $derived.by(() => {
		const q = linkModalUrl.trim().toLowerCase();
		if (!q || q.startsWith('http://') || q.startsWith('https://') || q.startsWith('mailto:')) return [];
		return linkSuggestTitles.filter(e => e.title.toLowerCase().includes(q)).slice(0, 8);
	});
	let textContextMenu = $state<{ x: number; y: number; submenuLeft: boolean } | null>(null);
	let tableContextMenu = $state<{ x: number; y: number; hasStyling: boolean } | null>(null);
	let tablePickerOpen = $state(false);
	let tablePickerHover = $state({ rows: 0, cols: 0 });
	let imageToolbar = $state<{ pos: number; x: number; y: number; size: string; src: string } | null>(null);
	let copyToast = $state<'copying' | 'done' | null>(null);

	// Math insert/edit modal (opened by /math slash command or double-click on existing math node)
	let mathModal = $state<{ kind: 'block' | 'inline'; editPos: number | null; tex: string } | null>(null);

	function openMathInsert(kind: 'block' | 'inline') {
		mathModal = { kind, editPos: null, tex: '' };
	}
	function openMathEdit(pos: number, kind: 'block' | 'inline', tex: string) {
		mathModal = { kind, editPos: pos, tex };
	}
	function cancelMathModal() {
		mathModal = null;
	}
	function commitMathModal() {
		if (!editor || !mathModal) return;
		const { kind, editPos, tex } = mathModal;
		const trimmed = tex.trim();
		mathModal = null;
		if (!trimmed) return;
		if (editPos !== null) {
			const node = editor.state.doc.nodeAt(editPos);
			if (node && (node.type.name === 'mathBlock' || node.type.name === 'mathInline')) {
				const tr = editor.state.tr.setNodeAttribute(editPos, 'tex', trimmed);
				editor.view.dispatch(tr);
			}
		} else {
			const nodeType = kind === 'block' ? 'mathBlock' : 'mathInline';
			editor.chain().focus().insertContent({ type: nodeType, attrs: { tex: trimmed } }).run();
		}
	}

	type SecretModalState = {
		title: string;
		text: string;
		passphrase: string;
		confirmPassphrase: string;
		from: number;
		to: number;
		error: string;
		busy: boolean;
	};

	let secretModal = $state<SecretModalState | null>(null);
	let secretTitleInput = $state<HTMLInputElement | null>(null);

	function selectedPlainText(): string {
		if (!editor) return '';
		const { from, to } = editor.state.selection;
		return from === to ? '' : editor.state.doc.textBetween(from, to, '\n\n');
	}

	function openSecretInsert() {
		const selection = editor?.state.selection;
		secretModal = {
			title: 'Encrypted secret',
			text: selectedPlainText(),
			passphrase: '',
			confirmPassphrase: '',
			from: selection?.from ?? 0,
			to: selection?.to ?? 0,
			error: '',
			busy: false,
		};
		tick().then(() => {
			secretTitleInput?.focus();
			secretTitleInput?.select();
		});
	}

	function cancelSecretModal() {
		secretModal = null;
	}

	async function commitSecretModal() {
		if (!editor || !secretModal || secretModal.busy) return;
		if (!secretModal.text) {
			secretModal.error = 'Secret text is required.';
			return;
		}
		if (!secretModal.passphrase) {
			secretModal.error = 'Passphrase is required.';
			return;
		}
		if (secretModal.passphrase !== secretModal.confirmPassphrase) {
			secretModal.error = 'Passphrases do not match.';
			return;
		}
		secretModal.busy = true;
		secretModal.error = '';
		try {
			const payload = await encryptSecretText(secretModal.text, secretModal.passphrase, secretModal.title);
			const { from, to } = secretModal;
			secretModal = null;
			editor.chain().focus().setTextSelection({ from, to }).insertContent({ type: 'secretBlock', attrs: { payload } }).run();
		} catch (e: any) {
			if (secretModal) {
				secretModal.error = e?.message || 'Could not encrypt secret.';
				secretModal.busy = false;
			}
		}
	}
	const katexCache = new Map<string, string>();
	function renderKatex(tex: string, displayMode: boolean): string {
		const key = (displayMode ? 'B:' : 'I:') + tex;
		let html = katexCache.get(key);
		if (html === undefined) {
			try {
				html = katex.renderToString(tex, { displayMode, throwOnError: false });
			} catch {
				html = `<span class="katex-error">${tex}</span>`;
			}
			katexCache.set(key, html);
		}
		return html;
	}

	let mathObserver: IntersectionObserver | null = null;
	const mathPending = new WeakMap<Element, () => void>();
	function observeMath(dom: HTMLElement, render: () => void) {
		if (!mathObserver) {
			const root = (editorElement?.closest('.editor-body') as Element) ?? null;
			mathObserver = new IntersectionObserver((entries) => {
				for (const e of entries) {
					if (!e.isIntersecting) continue;
					const fn = mathPending.get(e.target);
					if (fn) { mathPending.delete(e.target); mathObserver!.unobserve(e.target); fn(); }
				}
			}, { root, rootMargin: '1000px 0px' });
		}
		mathPending.set(dom, render);
		mathObserver.observe(dom);
	}

	function renderMathPreview(tex: string, displayMode: boolean): string {
		if (!tex.trim()) return '';
		try {
			return katex.renderToString(tex, { displayMode, throwOnError: false });
		} catch (e: any) {
			return `<span class="math-modal-preview-error">${e?.message || String(e)}</span>`;
		}
	}

	// External-file viewer mode UI
	let viewerImportPickerOpen = $state(false);
	let viewerImportBusy = $state(false);
	let viewerToast = $state<string | null>(null);
	let viewerFlatNotebooks = $derived.by(() => {
		if (!viewerImportPickerOpen) return [] as Array<{ path: string; name: string; depth: number }>;
		const out: Array<{ path: string; name: string; depth: number }> = [];
		const walk = (list: any[], depth: number) => {
			for (const nb of list) {
				out.push({ path: nb.path, name: nb.name, depth });
				if (nb.children?.length) walk(nb.children, depth + 1);
			}
		};
		walk($notebooks, 0);
		return out;
	});

	function viewerFlash(msg: string) {
		viewerToast = msg;
		setTimeout(() => { viewerToast = null; }, 1500);
	}

	function closeViewer() {
		$viewerNote = null;
		$activeNote = null;
		$activeNotePath = null;
		$readOnly = false;
		$focusMode = false;
	}

	async function viewerImportTo(folderPath: string) {
		const v = $viewerNote;
		const vaultRoot = $appConfig?.active_vault;
		if (!v || !vaultRoot || viewerImportBusy) return;
		viewerImportBusy = true;
		try {
			const filename = v.path.split('/').pop() || 'imported.md';
			const baseName = filename.replace(/\.md$/i, '');
			const folder = folderPath ? `${vaultRoot}/${folderPath}` : vaultRoot;
			let dest = `${folder}/${filename}`;
			// Conflict resolution: append (2), (3)... if file exists
			let n = 2;
			// readNote throws if file doesn't exist; use it as an existence probe
			while (true) {
				try { await readNote(dest); } catch { break; }
				dest = `${folder}/${baseName} (${n}).md`;
				n++;
				if (n > 100) throw new Error('Could not find a free filename');
			}
			await copyFileTo(v.path, dest);
			// Switch to the imported note as a real vault note
			$viewerNote = null;
			$readOnly = false;
			$focusMode = false;
			viewerImportPickerOpen = false;
			const content = await readNote(dest);
			$activeNote = content;
			$activeNotePath = dest;
			$editorDirty = false;
			loadNote(dest, content.content);
			viewerFlash('Imported');
		} catch (e: any) {
			console.error('[Viewer] import failed', e);
			viewerFlash('Import failed: ' + (e?.message || String(e)));
		} finally {
			viewerImportBusy = false;
		}
	}
	// Normalize Windows backslashes to '/' so the prefix strip and folder split work cross-platform (issue #99).
	let noteRelativePath = $derived($activeNotePath && $appConfig?.active_vault ? $activeNotePath.replace(/\\/g, '/').replace($appConfig.active_vault.replace(/\\/g, '/') + '/', '') : '');
	let noteFolder = $derived(noteRelativePath ? noteRelativePath.substring(0, noteRelativePath.lastIndexOf('/')) : '');
	let isQuickAccess = $derived(noteRelativePath ? $quickAccessPaths.includes(noteRelativePath) : false);

	const lowlight = createLowlight(common);
	lowlight.register('powershell', powershell);
	const codeLanguages = [...lowlight.listLanguages(), 'mermaid'].sort();
	const mdit = MarkdownIt({ html: true, linkify: false, breaks: false })
		.use(markdownItMark)
		.use(markdownItSup)
		.use(markdownItSub);
	// Disable indented code blocks - tab-indented text should stay as text, not become code
	mdit.disable('code');

	function normalizePath(p: string): string {
		const parts = p.split('/');
		const resolved: string[] = [];
		for (const seg of parts) {
			if (seg === '..') {
				resolved.pop();
			} else if (seg !== '.') {
				resolved.push(seg);
			}
		}
		return resolved.join('/');
	}

	const CustomImage = Image.extend({
		addAttributes() {
			return {
				...this.parent?.(),
				size: {
					default: 'full',
					parseHTML: (element: HTMLElement) => element.getAttribute('data-size') || 'full',
					renderHTML: (attributes: Record<string, any>) => {
						return { 'data-size': attributes.size };
					},
				},
			};
		},
	});

	function cellColorAttributes() {
		return {
			backgroundColor: {
				default: null,
				parseHTML: (element: HTMLElement) => element.getAttribute('data-bg-color') || element.style.backgroundColor || null,
				renderHTML: (attributes: Record<string, any>) => {
					if (!attributes.backgroundColor) return {};
					const bg = attributes.backgroundColor;
					// Determine if we need light text for dark backgrounds
					const darkBgs = ['#1e293b', '#374151', '#7f1d1d', '#713f12', '#14532d', '#1e3a5f', '#4c1d95', '#831843', '#0c4a6e', '#064e3b'];
					const needsLight = darkBgs.includes(bg);
					const style = needsLight
						? `background-color: ${bg}; color: #f1f5f9`
						: `background-color: ${bg}`;
					return { style, 'data-bg-color': bg };
				},
			},
		};
	}

	const CustomTableCell = TableCell.extend({
		addAttributes() {
			return { ...this.parent?.(), ...cellColorAttributes() };
		},
	});

	const CustomTableHeader = TableHeader.extend({
		addAttributes() {
			return { ...this.parent?.(), ...cellColorAttributes() };
		},
	});

	const PdfEmbed = TiptapNode.create({
		name: 'pdfEmbed',
		group: 'block',
		atom: true,
		addAttributes() {
			return {
				src: { default: null },
				name: { default: 'file.pdf' },
			};
		},
		parseHTML() {
			return [{
				tag: 'div[data-pdf-src]',
				getAttrs: (el: HTMLElement) => ({
					src: el.getAttribute('data-pdf-src'),
					name: el.getAttribute('data-pdf-name') || 'file.pdf',
				}),
			}];
		},
		renderHTML({ HTMLAttributes }) {
			const src = HTMLAttributes.src || '';
			const name = HTMLAttributes.name || 'file.pdf';
			const showInline = !isMobile && ($appConfig?.pdf_preview ?? false);
			if (showInline) {
				const vaultRoot = $appConfig?.active_vault ?? '';
				const pdfHeight = $appConfig?.pdf_height ?? 600;
				const absPath = normalizePath(`${vaultRoot}/${decodeURIComponent(src)}`);
				const displaySrc = convertFileSrc(absPath);
				return ['div', mergeAttributes({ 'data-pdf-src': src, 'data-pdf-name': name, class: 'pdf-embed' }),
					['iframe', { src: displaySrc, width: '100%', height: `${pdfHeight}px`, frameborder: '0' }],
					['p', { class: 'pdf-label' }, name],
				];
			}
			// Non-inline: render as a clickable link (mobile + desktop with setting off)
			return ['div', mergeAttributes({ 'data-pdf-src': src, 'data-pdf-name': name, class: 'pdf-embed-mobile' }),
				['a', { href: decodeURIComponent(src), class: 'pdf-link-mobile' },
					['span', { class: 'pdf-icon-mobile' }, '\uD83D\uDCC4'],
					['span', {}, name],
				],
			];
		},
	});

	function renderSecretNode(dom: HTMLElement, payload: string) {
		dom.textContent = '';
		const secretTitle = readSecretTitle(payload);

		const header = document.createElement('div');
		header.className = 'secret-block-header';

		const title = document.createElement('span');
		title.className = 'secret-block-title';
		title.innerHTML = '<svg class="secret-block-lock" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="10" rx="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>';
		title.append(document.createTextNode(secretTitle));

		const badge = document.createElement('span');
		badge.className = 'secret-block-badge';
		badge.textContent = 'AES-GCM';

		header.append(title, badge);

		const form = document.createElement('form');
		form.className = 'secret-block-form';

		const input = document.createElement('input');
		input.type = 'password';
		input.placeholder = 'Passphrase';
		input.autocomplete = 'current-password';
		input.size = 1;

		const unlock = document.createElement('button');
		unlock.type = 'submit';
		unlock.textContent = 'Unlock';

		const error = document.createElement('div');
		error.className = 'secret-block-error';

		form.append(input, unlock);
		dom.append(header, form, error);

		form.addEventListener('submit', async (event) => {
			event.preventDefault();
			error.textContent = '';
			unlock.disabled = true;
			try {
				const plaintext = await decryptSecretText(payload, input.value);
				renderUnlockedSecretNode(dom, payload, plaintext);
			} catch (e: any) {
				error.textContent = e?.message || 'Unable to unlock secret.';
				unlock.disabled = false;
			}
		});
	}

	function renderUnlockedSecretNode(dom: HTMLElement, payload: string, plaintext: string) {
		dom.textContent = '';
		const secretTitle = readSecretTitle(payload);

		const header = document.createElement('div');
		header.className = 'secret-block-header';

		const title = document.createElement('span');
		title.className = 'secret-block-title';
		title.textContent = secretTitle;

		const actions = document.createElement('div');
		actions.className = 'secret-block-actions';

		const copy = document.createElement('button');
		copy.type = 'button';
		copy.textContent = 'Copy';
		copy.addEventListener('click', async () => {
			await navigator.clipboard.writeText(plaintext);
			copy.textContent = 'Copied';
			setTimeout(() => { copy.textContent = 'Copy'; }, 1200);
		});

		const lock = document.createElement('button');
		lock.type = 'button';
		lock.textContent = 'Lock';
		lock.addEventListener('click', () => renderSecretNode(dom, payload));

		actions.append(copy, lock);
		header.append(title, actions);

		const body = document.createElement('pre');
		body.className = 'secret-block-plaintext';
		body.textContent = plaintext;

		dom.append(header, body);
	}

	function decodeSecretPayload(value: string): string {
		try {
			return decodeURIComponent(value);
		} catch {
			return '';
		}
	}

	const SecretBlock = TiptapNode.create({
		name: 'secretBlock',
		group: 'block',
		atom: true,
		selectable: true,
		addAttributes() {
			return { payload: { default: '' } };
		},
		parseHTML() {
			return [{
				tag: 'div[data-secret-block]',
				getAttrs: (el: HTMLElement) => ({
					payload: decodeSecretPayload(el.getAttribute('data-secret-block') || ''),
				}),
			}];
		},
		renderHTML({ HTMLAttributes }) {
			const payload = HTMLAttributes.payload || '';
			return ['div', { 'data-secret-block': encodeURIComponent(payload), class: 'secret-block' }];
		},
		addNodeView() {
			return ({ node }) => {
				let payload = node.attrs.payload || '';
				const dom = document.createElement('div');
				dom.className = 'secret-block';
				dom.contentEditable = 'false';
				renderSecretNode(dom, payload);
				return {
					dom,
					update(updatedNode) {
						if (updatedNode.type.name !== 'secretBlock') return false;
						payload = updatedNode.attrs.payload || '';
						renderSecretNode(dom, payload);
						return true;
					},
					stopEvent: () => true,
					ignoreMutation: () => true,
				};
			};
		},
	});

	const MathBlock = TiptapNode.create({
		name: 'mathBlock',
		group: 'block',
		atom: true,
		addAttributes() {
			return { tex: { default: '' } };
		},
		parseHTML() {
			return [{
				tag: 'div[data-math-block]',
				getAttrs: (el: HTMLElement) => ({ tex: decodeURIComponent(el.getAttribute('data-math-block') || '') }),
			}];
		},
		renderHTML({ HTMLAttributes }) {
			const tex = HTMLAttributes.tex || '';
			const rendered = renderKatex(tex, true);
			return ['div', { 'data-math-block': encodeURIComponent(tex), class: 'math-block', contenteditable: 'false' }, ['div', { innerHTML: rendered }]];
		},
		addNodeView() {
			return ({ node, getPos }) => {
				const dom = document.createElement('div');
				dom.classList.add('math-block');
				dom.contentEditable = 'false';
				dom.setAttribute('data-math-block', encodeURIComponent(node.attrs.tex));
				const render = () => { dom.innerHTML = renderKatex(node.attrs.tex, true); };
				if (isLargeDoc) { dom.textContent = node.attrs.tex; observeMath(dom, render); } else { render(); }
				dom.addEventListener('dblclick', (e) => {
					e.preventDefault();
					e.stopPropagation();
					const pos = typeof getPos === 'function' ? getPos() : null;
					if (pos !== null && pos !== undefined) openMathEdit(pos, 'block', node.attrs.tex);
				});
				return { dom, destroy() { mathObserver?.unobserve(dom); mathPending.delete(dom); } };
			};
		},
	});

	const MathInline = TiptapNode.create({
		name: 'mathInline',
		group: 'inline',
		inline: true,
		atom: true,
		addAttributes() {
			return { tex: { default: '' } };
		},
		parseHTML() {
			return [{
				tag: 'span[data-math-inline]',
				getAttrs: (el: HTMLElement) => ({ tex: decodeURIComponent(el.getAttribute('data-math-inline') || '') }),
			}];
		},
		renderHTML({ HTMLAttributes }) {
			const tex = HTMLAttributes.tex || '';
			const rendered = renderKatex(tex, false);
			return ['span', { 'data-math-inline': encodeURIComponent(tex), class: 'math-inline', contenteditable: 'false' }, ['span', { innerHTML: rendered }]];
		},
		addNodeView() {
			return ({ node, getPos }) => {
				const dom = document.createElement('span');
				dom.classList.add('math-inline');
				dom.contentEditable = 'false';
				dom.setAttribute('data-math-inline', encodeURIComponent(node.attrs.tex));
				const render = () => { dom.innerHTML = renderKatex(node.attrs.tex, false); };
				if (isLargeDoc) { dom.textContent = node.attrs.tex; observeMath(dom, render); } else { render(); }
				dom.addEventListener('dblclick', (e) => {
					e.preventDefault();
					e.stopPropagation();
					const pos = typeof getPos === 'function' ? getPos() : null;
					if (pos !== null && pos !== undefined) openMathEdit(pos, 'inline', node.attrs.tex);
				});
				return { dom, destroy() { mathObserver?.unobserve(dom); mathPending.delete(dom); } };
			};
		},
	});

	const Callout = TiptapNode.create({
		name: 'callout',
		group: 'block',
		content: 'block+',
		defining: true,
		addAttributes() {
			return {
				type: {
					default: 'note',
					parseHTML: (el: HTMLElement) => (el.getAttribute('data-callout') || 'note').toLowerCase(),
					renderHTML: (a: Record<string, any>) => ({ 'data-callout': a.type }),
				},
				title: {
					default: '',
					parseHTML: (el: HTMLElement) => el.getAttribute('data-callout-title') || '',
					renderHTML: (a: Record<string, any>) => (a.title ? { 'data-callout-title': a.title } : {}),
				},
				foldable: {
					default: false,
					parseHTML: (el: HTMLElement) => el.getAttribute('data-callout-foldable') === 'true',
					renderHTML: (a: Record<string, any>) => ({ 'data-callout-foldable': a.foldable ? 'true' : 'false' }),
				},
				folded: {
					default: false,
					parseHTML: (el: HTMLElement) => el.getAttribute('data-callout-folded') === 'true',
					renderHTML: (a: Record<string, any>) => ({ 'data-callout-folded': a.folded ? 'true' : 'false' }),
				},
			};
		},
		parseHTML() {
			return [{ tag: 'div[data-callout]' }];
		},
		renderHTML({ node, HTMLAttributes }) {
			return ['div', mergeAttributes(HTMLAttributes, { class: 'callout', 'data-callout-group': calloutGroup(node.attrs.type) }), 0];
		},
		addNodeView() {
			return ({ node, getPos, editor }) => {
				const dom = document.createElement('div');
				const apply = (n: any) => {
					dom.className = 'callout';
					dom.classList.toggle('is-foldable', !!n.attrs.foldable);
					dom.classList.toggle('is-folded', !!n.attrs.folded);
					dom.setAttribute('data-callout', n.attrs.type || 'note');
					dom.setAttribute('data-callout-group', calloutGroup(n.attrs.type));
				};
				apply(node);

				const header = document.createElement('div');
				header.className = 'callout-header';
				header.contentEditable = 'false';

				const iconBtn = document.createElement('button');
				iconBtn.type = 'button';
				iconBtn.className = 'callout-icon';
				iconBtn.title = 'Change type';
				iconBtn.innerHTML = calloutIcon(node.attrs.type);

				const titleInput = document.createElement('input');
				titleInput.className = 'callout-title';
				titleInput.value = node.attrs.title || '';
				titleInput.placeholder = calloutLabel(node.attrs.type);
				titleInput.spellcheck = false;
				titleInput.readOnly = !editor.isEditable;

				const foldBtn = document.createElement('button');
				foldBtn.type = 'button';
				foldBtn.className = 'callout-fold';
				foldBtn.title = 'Fold / unfold';
				foldBtn.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>';

				const content = document.createElement('div');
				content.className = 'callout-content';

				header.append(iconBtn, titleInput, foldBtn);
				dom.append(header, content);

				const updateAttr = (attrs: Record<string, any>) => {
					if (typeof getPos !== 'function') return;
					const pos = getPos();
					if (pos == null) return;
					const cur = editor.state.doc.nodeAt(pos);
					if (!cur) return;
					editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { ...cur.attrs, ...attrs }));
				};

				let titleDirty = false;
				const commitTitle = () => { if (titleDirty) { updateAttr({ title: titleInput.value }); titleDirty = false; } };
				titleInput.addEventListener('input', () => { titleDirty = true; });
				titleInput.addEventListener('change', commitTitle);
				titleInput.addEventListener('blur', commitTitle);
				titleInput.addEventListener('keydown', (e) => {
					if (e.key === 'Enter') { e.preventDefault(); commitTitle(); titleInput.blur(); editor.commands.focus(); }
				});

				foldBtn.addEventListener('mousedown', (e) => e.preventDefault());
				foldBtn.addEventListener('click', (e) => {
					e.preventDefault();
					const nowFolded = !dom.classList.contains('is-folded');
					if (editor.isEditable) {
						updateAttr({ folded: nowFolded, foldable: true });
					} else {
						dom.classList.toggle('is-folded', nowFolded);
						dom.classList.add('is-foldable');
					}
				});

				iconBtn.addEventListener('mousedown', (e) => e.preventDefault());
				iconBtn.addEventListener('click', (e) => {
					e.preventDefault();
					if (!editor.isEditable) return;
					openCalloutTypeMenu(iconBtn, (newType) => updateAttr({ type: newType }));
				});

				return {
					dom,
					contentDOM: content,
					update(updated: any) {
						if (updated.type.name !== 'callout') return false;
						apply(updated);
						iconBtn.innerHTML = calloutIcon(updated.attrs.type);
						titleInput.placeholder = calloutLabel(updated.attrs.type);
						titleInput.readOnly = !editor.isEditable;
						if (document.activeElement !== titleInput) titleInput.value = updated.attrs.title || '';
						return true;
					},
					ignoreMutation(mutation: any) {
						if (mutation.type === 'selection') return false;
						return !content.contains(mutation.target as Node);
					},
					stopEvent(event: any) {
						return header.contains(event.target as Node);
					},
				};
			};
		},
	});

	const HeadingShortcuts = Extension.create({
		name: 'headingShortcuts',
		addKeyboardShortcuts() {
			const toggle = (level: 1 | 2 | 3 | 4 | 5 | 6) => () =>
				this.editor.chain().focus().toggleHeading({ level }).run();
			return {
				'Mod-1': toggle(1),
				'Mod-2': toggle(2),
				'Mod-3': toggle(3),
				'Mod-4': toggle(4),
				'Mod-5': toggle(5),
				'Mod-6': toggle(6),
				'Mod-0': () => this.editor.chain().focus().setParagraph().run(),
			};
		},
	});

	const CtrlEndScrollPastEnd = Extension.create({
		name: 'ctrlEndScrollPastEnd',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('ctrlEndScrollPastEnd'),
					props: {
						handleDOMEvents: {
							keydown(view, event) {
								if (!event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || event.key !== 'End') return false;
								event.preventDefault();
								const tr = view.state.tr.setSelection(TextSelection.atEnd(view.state.doc));
								view.dispatch(tr);
								(view.dom as HTMLElement).focus({ preventScroll: true });
								scrollEditorBodyToBottom(view.dom as HTMLElement);
								return true;
							},
						},
					},
				}),
			];
		},
	});

	const PageBreak = TiptapNode.create({
		name: 'pageBreak',
		group: 'block',
		atom: true,
		parseHTML() {
			return [
				{ tag: 'div[data-page-break]' },
				{
					tag: 'div',
					getAttrs: (el: HTMLElement) => {
						const style = el.getAttribute('style') || '';
						return style.includes('page-break-after') ? {} : false;
					},
				},
			];
		},
		renderHTML() {
			return ['div', { 'data-page-break': 'true', style: 'page-break-after: always; break-after: page;', class: 'page-break' }];
		},
	});

	// Remap decorations while typing; full rebuild 300ms after it settles, so large notes don't rescan the whole doc per keystroke.
	function lazyDecorationPlugin(key: PluginKey, build: (doc: any) => DecorationSet) {
		return new Plugin({
			key,
			state: {
				init: (_config: any, state: any) => build(state.doc),
				apply: (tr: any, old: DecorationSet, _oldState: any, newState: any) => {
					if (tr.getMeta(key) === 'rebuildDecos') return build(newState.doc);
					if (!tr.docChanged) return old;
					return old.map(tr.mapping, tr.doc);
				},
			},
			props: {
				decorations(state: any) { return key.getState(state); },
			},
			view() {
				let timer: ReturnType<typeof setTimeout> | null = null;
				return {
					update(view: any, prev: any) {
						if (view.state.doc === prev.doc) return;
						if (timer) clearTimeout(timer);
						timer = setTimeout(() => {
							timer = null;
							if (!view.isDestroyed) view.dispatch(view.state.tr.setMeta(key, 'rebuildDecos'));
						}, 300);
					},
					destroy() { if (timer) clearTimeout(timer); },
				};
			},
		});
	}

	const MermaidRenderer = Extension.create({
		name: 'mermaidRendererOptIn',
		addProseMirrorPlugins() {
			console.info('[HelixNotes] Mermaid renderer (opt-in) initialised');

			let mermaidPromise: Promise<any> | null = null;
			const svgCache = new Map<string, string>();
			let renderCounter = 0;

			function loadMermaid(): Promise<any> {
				if (!mermaidPromise) {
					mermaidPromise = import('mermaid')
						.then((m) => {
							const lib = m.default;
							const isDark = document.documentElement.classList.contains('dark');
							lib.initialize({
								startOnLoad: false,
								theme: isDark ? 'dark' : 'default',
								securityLevel: 'strict',
								fontFamily: 'inherit',
							});
							return lib;
						})
						.catch((e) => { console.error('[Mermaid] load failed', e); return null; });
				}
				return mermaidPromise;
			}

			function showError(container: HTMLElement, msg: string) {
				container.innerHTML = '';
				container.classList.remove('mermaid-render-loading');
				container.classList.add('mermaid-render-error');
				const text = document.createElement('div');
				text.textContent = msg;
				container.appendChild(text);
				addRetryButton(container, container.getAttribute('data-mermaid-source') || '');
			}

			function addRetryButton(container: HTMLElement, source: string) {
				const btn = document.createElement('button');
				btn.type = 'button';
				btn.className = 'mermaid-render-btn mermaid-render-btn-small';
				btn.textContent = '↻ Retry';
				btn.onclick = (e) => {
					e.preventDefault();
					e.stopPropagation();
					renderInto(container, source);
				};
				container.appendChild(btn);
			}

			async function svgToPngBlob(svgEl: SVGElement, scale = 2): Promise<Blob> {
				const clone = svgEl.cloneNode(true) as SVGElement;
				if (!clone.getAttribute('xmlns')) clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
				const svgString = new XMLSerializer().serializeToString(clone);
				const svgBlob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' });
				const url = URL.createObjectURL(svgBlob);
				try {
					const img = new window.Image();
					await new Promise<void>((resolve, reject) => {
						img.onload = () => resolve();
						img.onerror = () => reject(new Error('SVG image load failed'));
						img.src = url;
					});
					const bbox = svgEl.getBoundingClientRect();
					const width = Math.max(Math.round(bbox.width || 800), 100);
					const height = Math.max(Math.round(bbox.height || 600), 100);
					const canvas = document.createElement('canvas');
					canvas.width = width * scale;
					canvas.height = height * scale;
					const ctx = canvas.getContext('2d');
					if (!ctx) throw new Error('Canvas not supported');
					ctx.scale(scale, scale);
					ctx.fillStyle = document.documentElement.classList.contains('dark') ? '#1e1e1e' : '#ffffff';
					ctx.fillRect(0, 0, width, height);
					ctx.drawImage(img, 0, 0, width, height);
					return await new Promise<Blob>((resolve, reject) => {
						canvas.toBlob((b) => b ? resolve(b) : reject(new Error('toBlob failed')), 'image/png');
					});
				} finally {
					URL.revokeObjectURL(url);
				}
			}

			function flashToast(container: HTMLElement, msg: string) {
				const existing = container.querySelector('.mermaid-render-toast');
				if (existing) existing.remove();
				const toast = document.createElement('div');
				toast.className = 'mermaid-render-toast';
				toast.textContent = msg;
				container.appendChild(toast);
				setTimeout(() => { if (toast.parentElement) toast.remove(); }, 1500);
			}

			async function copyDiagram(container: HTMLElement) {
				const svgEl = container.querySelector('svg') as SVGElement | null;
				if (!svgEl) return;
				try {
					const blob = await svgToPngBlob(svgEl);
					const buf = new Uint8Array(await blob.arrayBuffer());
					await copyPngToClipboard(buf);
					flashToast(container, 'Copied');
				} catch (e: any) {
					console.error('[Mermaid] copy failed', e);
					flashToast(container, 'Copy failed: ' + (e?.message || String(e)));
				}
			}

			async function saveDiagram(container: HTMLElement) {
				const svgEl = container.querySelector('svg') as SVGElement | null;
				if (!svgEl) return;
				try {
					const dest = await saveDialog({
						defaultPath: 'diagram.png',
						filters: [
							{ name: 'PNG Image', extensions: ['png'] },
							{ name: 'SVG Image', extensions: ['svg'] },
						],
					});
					if (!dest) return;
					const lower = dest.toLowerCase();
					if (lower.endsWith('.svg')) {
						const clone = svgEl.cloneNode(true) as SVGElement;
						if (!clone.getAttribute('xmlns')) clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
						const svgString = new XMLSerializer().serializeToString(clone);
						await writeBytesTo(dest, new TextEncoder().encode(svgString));
					} else {
						const blob = await svgToPngBlob(svgEl);
						const buf = new Uint8Array(await blob.arrayBuffer());
						await writeBytesTo(dest, buf);
					}
					flashToast(container, 'Saved');
				} catch (e: any) {
					console.error('[Mermaid] save failed', e);
					flashToast(container, 'Save failed: ' + (e?.message || String(e)));
				}
			}

			function addToolbar(container: HTMLElement, source: string) {
				const toolbar = document.createElement('div');
				toolbar.className = 'mermaid-render-toolbar';

				if (!isAndroid) {
					const copyBtn = document.createElement('button');
					copyBtn.type = 'button';
					copyBtn.className = 'mermaid-render-action';
					copyBtn.title = 'Copy as PNG';
					copyBtn.textContent = 'Copy';
					copyBtn.onclick = (e) => { e.preventDefault(); e.stopPropagation(); copyDiagram(container); };
					toolbar.appendChild(copyBtn);
				}

				const saveBtn = document.createElement('button');
				saveBtn.type = 'button';
				saveBtn.className = 'mermaid-render-action';
				saveBtn.title = 'Save as PNG or SVG';
				saveBtn.textContent = 'Save';
				saveBtn.onclick = (e) => { e.preventDefault(); e.stopPropagation(); saveDiagram(container); };

				const reRenderBtn = document.createElement('button');
				reRenderBtn.type = 'button';
				reRenderBtn.className = 'mermaid-render-action';
				reRenderBtn.title = 'Re-render diagram';
				reRenderBtn.textContent = '↻';
				reRenderBtn.onclick = (e) => {
					e.preventDefault();
					e.stopPropagation();
					svgCache.delete(source);
					renderInto(container, source);
				};

				toolbar.appendChild(saveBtn);
				toolbar.appendChild(reRenderBtn);
				container.appendChild(toolbar);
			}

			async function renderInto(container: HTMLElement, source: string) {
				container.innerHTML = '';
				container.classList.remove('mermaid-render-error', 'mermaid-render-idle');
				container.classList.add('mermaid-render-loading');

				const cached = svgCache.get(source);
				if (cached) {
					container.classList.remove('mermaid-render-loading');
					container.innerHTML = cached;
					addToolbar(container, source);
					return;
				}

				const mermaid = await loadMermaid();
				if (!mermaid) {
					showError(container, 'Mermaid library failed to load.');
					return;
				}
				try {
					const parseOk = await mermaid.parse(source, { suppressErrors: true });
					if (!parseOk) {
						showError(container, 'Invalid mermaid syntax.');
						return;
					}
					const id = `mermaid-${++renderCounter}`;
					const { svg } = await mermaid.render(id, source);
					svgCache.set(source, svg);
					if (container.isConnected) {
						container.classList.remove('mermaid-render-loading');
						container.innerHTML = svg;
						addToolbar(container, source);
					}
				} catch (e: any) {
					showError(container, 'Render failed: ' + (e?.message || String(e)));
				}
			}

			function makeIdleButton(source: string): HTMLElement {
				const container = document.createElement('div');
				container.className = 'mermaid-render mermaid-render-idle';
				container.contentEditable = 'false';
				container.setAttribute('data-mermaid-source', source);

				const cached = svgCache.get(source);
				if (cached) {
					container.classList.remove('mermaid-render-idle');
					container.innerHTML = cached;
					addToolbar(container, source);
					return container;
				}

				const btn = document.createElement('button');
				btn.type = 'button';
				btn.className = 'mermaid-render-btn';
				btn.textContent = '▶  Render diagram';
				btn.onclick = (e) => {
					e.preventDefault();
					e.stopPropagation();
					renderInto(container, source);
				};
				container.appendChild(btn);
				return container;
			}

			function buildDecorations(doc: any): DecorationSet {
				const decos: any[] = [];
				doc.descendants((node: any, pos: number) => {
					if (node.type.name === 'codeBlock' && node.attrs.language === 'mermaid') {
						const source = node.textContent;
						if (!source.trim()) return;
						decos.push(
							Decoration.widget(
								pos + node.nodeSize,
								() => makeIdleButton(source),
								{ side: 1, key: 'mermaid:' + source.length + ':' + (svgCache.has(source) ? 'r' : 'i') },
							),
						);
					}
				});
				return DecorationSet.create(doc, decos);
			}

			const pluginKey = new PluginKey('mermaidRendererOptIn');
			return [lazyDecorationPlugin(pluginKey, buildDecorations)];
		},
	});

	const COPY_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
	const CHECK_ICON = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;

	const CopyButtonExtension = Extension.create({
		name: 'codeBlockCopyButton',
		addProseMirrorPlugins() {
			function buildDecorations(doc: any): DecorationSet {
				const decos: any[] = [];
				doc.descendants((node: any, pos: number) => {
					if (node.type.name === 'codeBlock') {
						const btn = document.createElement('button');
						btn.className = 'code-copy-btn';
						btn.title = 'Copy code';
						btn.innerHTML = COPY_ICON;
						btn.addEventListener('click', (e) => {
							e.preventDefault();
							e.stopPropagation();
							navigator.clipboard.writeText(node.textContent).then(() => {
								btn.innerHTML = CHECK_ICON;
								btn.classList.add('copied');
								setTimeout(() => {
									btn.innerHTML = COPY_ICON;
									btn.classList.remove('copied');
								}, 1500);
							});
						});
						decos.push(Decoration.widget(pos + 1, btn, { side: -1, key: `copy-btn:${pos}` }));
					}
				});
				return DecorationSet.create(doc, decos);
			}
			const pluginKey = new PluginKey('codeBlockCopyButton');
			return [lazyDecorationPlugin(pluginKey, buildDecorations)];
		},
	});

	let codeLangDropdown = $state<{ pos: number; x: number; y: number; current: string } | null>(null);
	let codeLangSearch = $state('');
	let codeLangInput = $state<HTMLInputElement | null>(null);

	let codeLangFiltered = $derived.by(() => {
		if (!codeLangSearch) return codeLanguages;
		const q = codeLangSearch.toLowerCase();
		return codeLanguages.filter(l => l.includes(q));
	});

	function openCodeLangDropdown(pos: number, current: string, triggerEl: HTMLElement) {
		const rect = triggerEl.getBoundingClientRect();
		codeLangSearch = '';
		codeLangDropdown = { pos, x: rect.right, y: rect.bottom + 4, current };
		tick().then(() => codeLangInput?.focus());
	}

	function selectCodeLang(lang: string) {
		if (!editor || !codeLangDropdown) return;
		const { pos } = codeLangDropdown;
		// Find the codeBlock node at this position
		const resolved = editor.state.doc.resolve(pos);
		const node = resolved.parent;
		if (node.type.name === 'codeBlock') {
			editor.chain().focus().updateAttributes('codeBlock', { language: lang || null }).run();
		}
		codeLangDropdown = null;
	}

	function closeCodeLangDropdown() {
		codeLangDropdown = null;
	}

	// ── In-note search extension ──
	const NoteSearchExtension = Extension.create({
		name: 'noteSearch',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: noteSearchPluginKey,
					state: {
						init() { return DecorationSet.empty; },
						apply(tr, old) {
							const meta = tr.getMeta(noteSearchPluginKey);
							if (meta !== undefined) return meta;
							return old.map(tr.mapping, tr.doc);
						},
					},
					props: {
						decorations(state) {
							return this.getState(state);
						},
					},
				}),
			];
		},
	});

	// Color swatch decorations: render a small filled square before every hex/rgb/hsl color
	// literal (in normal text AND code blocks), VSCode-style. These are view-only widget
	// decorations, so they never touch the document/markdown - the note stores only the plain
	// color text and the swatch is re-derived on load.
	const colorSwatchPluginKey = new PluginKey('colorSwatch');
	const COLOR_LITERAL_RE = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b|(?:rgb|rgba|hsl|hsla)\([^)\n]{1,64}\)/g;

	function makeColorSwatch(color: string): HTMLElement {
		const span = document.createElement('span');
		span.className = 'color-swatch';
		span.contentEditable = 'false';
		span.style.backgroundColor = color;
		return span;
	}

	function buildColorSwatchDecorations(doc: any): DecorationSet {
		const decos: any[] = [];
		doc.descendants((node: any, pos: number) => {
			if (!node.isText || !node.text) return;
			const text: string = node.text;
			COLOR_LITERAL_RE.lastIndex = 0;
			let m: RegExpExecArray | null;
			while ((m = COLOR_LITERAL_RE.exec(text)) !== null) {
				const color = m[0];
				// Validate with the browser's own CSS parser so junk (rgb(foo), #12345) gets no swatch.
				if (!CSS.supports('color', color)) continue;
				const at = pos + m.index;
				decos.push(Decoration.widget(at, () => makeColorSwatch(color), { side: -1, key: 'cs:' + color + '@' + at }));
			}
		});
		return DecorationSet.create(doc, decos);
	}

	const ColorSwatch = Extension.create({
		name: 'colorSwatch',
		addProseMirrorPlugins() {
			return [lazyDecorationPlugin(colorSwatchPluginKey, buildColorSwatchDecorations)];
		},
	});

	// Dim the inline task metadata tokens (!high / !medium / !med / !low and due:YYYY-MM-DD) inside
	// task items so they recede visually without disappearing. View-only inline decorations - the
	// note text is untouched. Token shapes mirror the parser in commands.rs.
	const taskMetaPluginKey = new PluginKey('taskMetaDim');
	const TASK_PRIO_RE = /(^|\s)(!(?:high|medium|med|low))\b/gi;
	const TASK_DUE_RE = /\bdue:\d{4}-\d{2}-\d{2}\b/gi;

	function buildTaskMetaDecorations(doc: any): DecorationSet {
		const decos: any[] = [];
		doc.descendants((node: any, pos: number) => {
			if (!node.isText || !node.text) return;
			// Only inside task items - elsewhere "!high" / "due:..." are ordinary prose.
			const rpos = doc.resolve(pos);
			let inTask = false;
			for (let d = rpos.depth; d > 0; d--) {
				if (rpos.node(d).type.name === 'taskItem') { inTask = true; break; }
			}
			if (!inTask) return;
			const text: string = node.text;
			let m: RegExpExecArray | null;
			TASK_PRIO_RE.lastIndex = 0;
			while ((m = TASK_PRIO_RE.exec(text)) !== null) {
				const start = pos + m.index + m[1].length;
				decos.push(Decoration.inline(start, start + m[2].length, { class: 'task-meta-dim' }));
			}
			TASK_DUE_RE.lastIndex = 0;
			while ((m = TASK_DUE_RE.exec(text)) !== null) {
				decos.push(Decoration.inline(pos + m.index, pos + m.index + m[0].length, { class: 'task-meta-dim' }));
			}
		});
		return DecorationSet.create(doc, decos);
	}

	const TaskMetaDim = Extension.create({
		name: 'taskMetaDim',
		addProseMirrorPlugins() {
			return [lazyDecorationPlugin(taskMetaPluginKey, buildTaskMetaDecorations)];
		},
	});

	const MoveLineShortcuts = Extension.create({
		name: 'moveLineShortcuts',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('moveLineShortcuts'),
					props: {
						handleDOMEvents: {
							keydown(view, event) {
								if (!event.altKey || (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')) return false;
								event.preventDefault();
								const { state, dispatch } = view;
								const resolvedPos = state.selection.$from;
								if (event.shiftKey) {
									for (let depth = resolvedPos.depth; depth > 0; depth--) {
										const itemNode = resolvedPos.node(depth);
										if (itemNode.type.name !== 'listItem' && itemNode.type.name !== 'taskItem') continue;
										const parentListDepth = depth - 1;
										const itemIndex = resolvedPos.index(parentListDepth);
										const itemPos = resolvedPos.before(depth);
										const itemSlice = state.doc.slice(itemPos, itemPos + itemNode.nodeSize);
										const cursorOffset = resolvedPos.pos - itemPos;
										const tr = state.tr;

										if (event.key === 'ArrowUp') {
											if (itemIndex <= 0) return true;
											const prevPos = resolvedPos.posAtIndex(itemIndex - 1, parentListDepth);
											tr.delete(itemPos, itemPos + itemNode.nodeSize);
											const insertAt = tr.mapping.map(prevPos);
											tr.insert(insertAt, itemSlice.content);
											const newCursorPos = Math.min(insertAt + cursorOffset, tr.doc.content.size);
											tr.setSelection(Selection.near(tr.doc.resolve(newCursorPos)));
											dispatch(tr.scrollIntoView());
											return true;
										}

										const parentList = resolvedPos.node(parentListDepth);
										if (itemIndex >= parentList.childCount - 1) return true;
										const nextPos = resolvedPos.posAtIndex(itemIndex + 1, parentListDepth);
										const nextNode = state.doc.nodeAt(nextPos);
										if (!nextNode) return true;
										const nextSlice = state.doc.slice(nextPos, nextPos + nextNode.nodeSize);
										tr.delete(nextPos, nextPos + nextNode.nodeSize);
										const insertAt = tr.mapping.map(itemPos);
										tr.insert(insertAt, nextSlice.content);
										const newCursorPos = Math.min(tr.mapping.map(itemPos) + cursorOffset, tr.doc.content.size);
										tr.setSelection(Selection.near(tr.doc.resolve(newCursorPos)));
										dispatch(tr.scrollIntoView());
										return true;
									}
									return true;
								}
								// Find the top-level block index
								const depth = 1; // top-level blocks in doc
								if (resolvedPos.depth < depth) return true;
								const parentPos = resolvedPos.before(depth);
								const parentNode = state.doc.nodeAt(parentPos);
								if (!parentNode) return true;
								const parentIndex = resolvedPos.index(0);
								if (event.key === 'ArrowUp') {
									if (parentIndex <= 0) return true;
									const prevPos = resolvedPos.posAtIndex(parentIndex - 1, 0);
									const prevNode = state.doc.nodeAt(prevPos);
									if (!prevNode) return true;
									const tr = state.tr;
									const cursorOffset = resolvedPos.pos - parentPos;
									// Delete current block, insert it before previous block
									const curSlice = state.doc.slice(parentPos, parentPos + parentNode.nodeSize);
									tr.delete(parentPos, parentPos + parentNode.nodeSize);
									const insertAt = tr.mapping.map(prevPos);
									tr.insert(insertAt, curSlice.content);
									const newCursorPos = Math.min(insertAt + cursorOffset, tr.doc.content.size);
									tr.setSelection(Selection.near(tr.doc.resolve(newCursorPos)));
									dispatch(tr.scrollIntoView());
								} else {
									if (parentIndex >= state.doc.childCount - 1) return true;
									const nextPos = resolvedPos.posAtIndex(parentIndex + 1, 0);
									const nextNode = state.doc.nodeAt(nextPos);
									if (!nextNode) return true;
									const tr = state.tr;
									const cursorOffset = resolvedPos.pos - parentPos;
									// Delete next block, insert it before current block
									const nextSlice = state.doc.slice(nextPos, nextPos + nextNode.nodeSize);
									tr.delete(nextPos, nextPos + nextNode.nodeSize);
									const insertAt = tr.mapping.map(parentPos);
									tr.insert(insertAt, nextSlice.content);
									const newCursorPos = Math.min(tr.mapping.map(parentPos) + cursorOffset, tr.doc.content.size);
									tr.setSelection(Selection.near(tr.doc.resolve(newCursorPos)));
									dispatch(tr.scrollIntoView());
								}
								return true;
							},
						},
					},
				}),
			];
		},
	});

	// Tab inserts a tab character in plain paragraphs/headings.
	// Priority 50 < default 100, so list/task/table/codeblock extensions handle Tab first for their own nodes.
	const TabIndent = Extension.create({
		name: 'tabIndent',
		priority: 50,
		addKeyboardShortcuts() {
			return {
				Tab: () => {
					const sel = this.editor.state.selection;
					const from = sel.$from;
					const node = from.node();
					if (node.type.name !== 'paragraph' && node.type.name !== 'heading') return false;
					// Don't intercept if inside a list or task item (their extensions handle Tab first,
					// but guard here too in case of priority edge cases)
					for (let d = from.depth - 1; d > 0; d--) {
						const name = from.node(d).type.name;
						if (name === 'listItem' || name === 'taskItem') return false;
					}
					return this.editor.commands.insertContent('\t');
				},
			};
		},
	});

	const CodeBlockLanguageSelect = Extension.create({
		name: 'codeBlockLanguageSelect',
		addGlobalAttributes() {
			return [{
				types: ['codeBlock'],
				attributes: {
					language: {
						renderHTML: (attributes) => {
							return { 'data-language': attributes.language || '' };
						},
					},
				},
			}];
		},
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('codeBlockLanguageSelect'),
					props: {
						handleDOMEvents: {
							click: (view, event) => {
								const target = event.target as HTMLElement;
								const pre = target.closest('pre');
								if (!pre) return false;
								// Check if click is in the top-right corner (language button area)
								const rect = pre.getBoundingClientRect();
								if (event.clientX < rect.right - 100 || event.clientY > rect.top + 30) return false;
								// Find the code block position
								const pos = view.posAtDOM(pre, 0);
								const resolved = view.state.doc.resolve(pos);
								let cbNode = resolved.parent;
								let cbPos = resolved.before(resolved.depth);
								if (cbNode.type.name !== 'codeBlock') {
									for (let d = resolved.depth; d >= 0; d--) {
										if (resolved.node(d).type.name === 'codeBlock') {
											cbNode = resolved.node(d);
											cbPos = resolved.before(d);
											break;
										}
									}
								}
								if (cbNode.type.name !== 'codeBlock') return false;
								event.preventDefault();
								event.stopPropagation();
								// Virtual trigger for dropdown positioning
								const triggerEl = document.createElement('div');
								triggerEl.getBoundingClientRect = () => ({
									top: rect.top + 5, bottom: rect.top + 25,
									left: rect.right - 100, right: rect.right - 34,
									width: 66, height: 20,
									x: rect.right - 100, y: rect.top + 5,
									toJSON() { return this; },
								});
								openCodeLangDropdown(cbPos + 1, cbNode.attrs.language || '', triggerEl as any);
								return true;
							},
						},
					},
				}),
			];
		},
	});

	function executeSlashCommand(index: number) {
		const items = slashFiltered;
		if (index < 0 || index >= items.length || !slashMenu || !editor) return;
		const cmd = items[index];
		// Table opens a sub-picker instead of closing
		if (cmd.label === 'Table') {
			slashTablePicker = true;
			slashTableHover = { rows: 0, cols: 0 };
			slashSelectedIndex = 0;
			// Delete slash text after setting flag so onTransaction doesn't close menu
			editor.chain().focus().deleteRange({ from: slashMenu.from, to: slashMenu.to }).run();
			return;
		}
		// Color opens a sub-picker too
		if (cmd.label === 'Color') {
			slashColorPicker = true;
			slashSelectedIndex = 0;
			editor.chain().focus().deleteRange({ from: slashMenu.from, to: slashMenu.to }).run();
			tick().then(() => slashColorInputEl?.focus());
			return;
		}
		// Delete the slash trigger text (/ + query)
		editor.chain().focus().deleteRange({ from: slashMenu.from, to: slashMenu.to }).run();
		slashMenu = null;
		slashSelectedIndex = 0;
		// Execute after the deletion is applied
		tick().then(() => cmd.action());
	}

	function slashInsertTable(rows: number, cols: number) {
		if (!editor) return;
		editor.chain().focus().insertTable({ rows, cols, withHeaderRow: true }).run();
		closeSlashMenu();
	}

	function insertColor(color: string) {
		if (!editor) return;
		const c = (color || '').trim();
		if (!c || !CSS.supports('color', c)) { closeSlashMenu(); return; }
		editor.chain().focus().insertContent(c).run();
		closeSlashMenu();
	}

	// Track whether the user just typed a slash (vs cursor moving into existing text)
	let slashTypedByUser = false;

	function closeSlashMenu() {
		slashMenu = null;
		slashSelectedIndex = 0;
		slashTablePicker = false;
		slashTableHover = { rows: 0, cols: 0 };
		slashColorPicker = false;
	}

	$effect(() => {
		if (!slashMenu || slashSelectedIndex < 0) return;
		slashSelectedIndex; // track
		tick().then(() => {
			document.querySelector('.slash-menu .slash-menu-item.selected')?.scrollIntoView({ block: 'nearest' });
		});
	});

	$effect(() => {
		if (!wikiLinkMenu || wikiLinkSelectedIndex < 0) return;
		wikiLinkSelectedIndex; // track
		tick().then(() => {
			document.querySelector('.wiki-link-menu .wiki-link-item.selected')?.scrollIntoView({ block: 'nearest' });
		});
	});

	function updateSlashMenu() {
		const wasSlashTyped = slashTypedByUser;
		slashTypedByUser = false;
		if (!editor) return;
		if (slashTablePicker || slashColorPicker) return; // a sub-picker is open, don't interfere
		const { state } = editor;
		const { selection } = state;
		const resolvedFrom = selection.$from;

		// Only in empty-ish context (paragraph, heading)
		const parentNode = resolvedFrom.parent;
		if (parentNode.type.name !== 'paragraph' && parentNode.type.name !== 'heading') {
			closeSlashMenu();
			return;
		}

		const textBefore = parentNode.textContent.slice(0, resolvedFrom.parentOffset);
		// Match "/" at start of line or after whitespace
		const match = textBefore.match(/(^|\s)\/([^\s]*)$/);
		if (!match) {
			closeSlashMenu();
			return;
		}

		// Only open the menu if the user typed the slash, or the menu is already open
		// This prevents triggering when clicking/arrowing into existing paths like /usr/local/bin
		if (!slashMenu && !wasSlashTyped) {
			return;
		}

		const query = match[2];
		const slashOffset = textBefore.length - match[0].length + (match[1].length); // position of "/"
		const from = resolvedFrom.start() + slashOffset;
		const to = resolvedFrom.pos;

		// Get cursor coordinates for menu positioning
		const coords = editor.view.coordsAtPos(from);

		let x = coords.left;

		// Keep menu within viewport (account for virtual keyboard on mobile)
		if (x + 240 > window.innerWidth) x = window.innerWidth - 250;
		const visibleBottom = window.innerHeight - keyboardHeight;
		const menuHeight = 300;
		let y = coords.bottom + 4;
		if (y + menuHeight > visibleBottom) y = coords.top - menuHeight - 4;
		if (y < 4) y = 4;

		slashMenu = { x, y, query, from, to };
		slashSelectedIndex = 0;
	}

	const SlashCommands = Extension.create({
		name: 'slashCommands',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('slashCommands'),
					props: {
						handleTextInput: (_view, _from, _to, text) => {
							if (text === '/') {
								slashTypedByUser = true;
							}
							return false;
						},
						handleKeyDown: (_view, event) => {
							if (!slashMenu) return false;
							if (slashColorPicker) {
								if (event.key === 'Escape') {
									event.preventDefault();
									closeSlashMenu();
								}
								return true; // the picker's own inputs handle the rest
							}
							if (slashTablePicker) {
								if (event.key === 'Escape') {
									event.preventDefault();
									closeSlashMenu();
									return true;
								}
								if (event.key === 'Tab') {
									event.preventDefault();
									if (slashTableHover.rows > 0 && slashTableHover.cols > 0) {
										slashInsertTable(slashTableHover.rows, slashTableHover.cols);
									}
									return true;
								}
								if (event.key === 'ArrowRight') {
									event.preventDefault();
									slashTableHover = { rows: Math.max(1, slashTableHover.rows), cols: Math.min(10, (slashTableHover.cols || 0) + 1) };
									return true;
								}
								if (event.key === 'ArrowLeft') {
									event.preventDefault();
									slashTableHover = { rows: Math.max(1, slashTableHover.rows), cols: Math.max(1, slashTableHover.cols - 1) };
									return true;
								}
								if (event.key === 'ArrowDown') {
									event.preventDefault();
									slashTableHover = { rows: Math.min(8, (slashTableHover.rows || 0) + 1), cols: Math.max(1, slashTableHover.cols) };
									return true;
								}
								if (event.key === 'ArrowUp') {
									event.preventDefault();
									slashTableHover = { rows: Math.max(1, slashTableHover.rows - 1), cols: Math.max(1, slashTableHover.cols) };
									return true;
								}
								if (event.key === 'Enter' || event.key === ' ') {
									event.preventDefault();
									if (slashTableHover.rows > 0 && slashTableHover.cols > 0) {
										slashInsertTable(slashTableHover.rows, slashTableHover.cols);
									}
									return true;
								}
								return true;
							}
							if (event.key === 'ArrowDown') {
								event.preventDefault();
								slashSelectedIndex = (slashSelectedIndex + 1) % Math.max(1, slashFiltered.length);
								return true;
							}
							if (event.key === 'ArrowUp') {
								event.preventDefault();
								slashSelectedIndex = (slashSelectedIndex - 1 + slashFiltered.length) % Math.max(1, slashFiltered.length);
								return true;
							}
							if (event.key === 'Enter' || event.key === 'Tab') {
								if (slashFiltered.length > 0) {
									event.preventDefault();
									executeSlashCommand(slashSelectedIndex);
									return true;
								}
								closeSlashMenu();
								return false;
							}
							if (event.key === 'Escape') {
								event.preventDefault();
								closeSlashMenu();
								return true;
							}
							return false;
						},
					},
				}),
			];
		},
	});

	// ── Task metadata (! priority / due) ──

	const TASK_META_ITEMS = [
		{ label: 'High priority', token: '!high', kind: 'prio', icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" y1="22" x2="4" y2="15"/></svg>' },
		{ label: 'Medium priority', token: '!med', kind: 'prio', icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" y1="22" x2="4" y2="15"/></svg>' },
		{ label: 'Low priority', token: '!low', kind: 'prio', icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z"/><line x1="4" y1="22" x2="4" y2="15"/></svg>' },
		{ label: 'Due date', token: '', kind: 'due', icon: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="18" rx="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>' },
	];
	let taskMetaMenu = $state<{ x: number; y: number; from: number; to: number; query: string } | null>(null);
	let taskMetaSelectedIndex = $state(0);
	let taskMetaTypedByUser = false;
	let taskDuePicker = $state<{ x: number; y: number } | null>(null);
	let taskDueInputEl = $state<HTMLInputElement | null>(null);

	let taskMetaFiltered = $derived.by(() => {
		const q = (taskMetaMenu?.query || '').toLowerCase();
		if (!q) return TASK_META_ITEMS;
		return TASK_META_ITEMS.filter((it) => it.label.toLowerCase().startsWith(q) || it.token.slice(1).startsWith(q));
	});

	function closeTaskMetaMenu() {
		taskMetaMenu = null;
		taskMetaSelectedIndex = 0;
	}

	function updateTaskMetaMenu() {
		const wasTyped = taskMetaTypedByUser;
		taskMetaTypedByUser = false;
		if (!editor || taskDuePicker) return;
		const rfrom = editor.state.selection.$from;
		let inTask = false;
		for (let d = rfrom.depth; d > 0; d--) {
			if (rfrom.node(d).type.name === 'taskItem') { inTask = true; break; }
		}
		if (!inTask || rfrom.parent.type.name !== 'paragraph') { closeTaskMetaMenu(); return; }
		const textBefore = rfrom.parent.textContent.slice(0, rfrom.parentOffset);
		const match = textBefore.match(/(^|\s)!([a-z]*)$/i);
		if (!match) { closeTaskMetaMenu(); return; }
		if (!taskMetaMenu && !wasTyped) return;
		const query = match[2];
		const offset = textBefore.length - match[0].length + match[1].length;
		const from = rfrom.start() + offset;
		const to = rfrom.pos;
		const coords = editor.view.coordsAtPos(from);
		let x = coords.left;
		if (x + 200 > window.innerWidth) x = window.innerWidth - 210;
		const visibleBottom = window.innerHeight - keyboardHeight;
		const menuH = 180;
		let y = coords.bottom + 4;
		if (y + menuH > visibleBottom) y = coords.top - menuH - 4;
		if (y < 4) y = 4;
		taskMetaMenu = { x, y, from, to, query };
		if (taskMetaSelectedIndex >= taskMetaFiltered.length) taskMetaSelectedIndex = 0;
	}

	function selectTaskMeta(index: number) {
		const items = taskMetaFiltered;
		if (index < 0 || index >= items.length || !taskMetaMenu || !editor) return;
		const item = items[index];
		const { from, to, x, y } = taskMetaMenu;
		taskMetaMenu = null;
		taskMetaSelectedIndex = 0;
		if (item.kind === 'due') {
			editor.chain().focus().deleteRange({ from, to }).run();
			taskDuePicker = { x, y };
			tick().then(() => { taskDueInputEl?.focus(); taskDueInputEl?.showPicker?.(); });
			return;
		}
		editor.chain().focus().deleteRange({ from, to }).insertContent(item.token + ' ').run();
	}

	function applyTaskDue(value: string) {
		taskDuePicker = null;
		if (!editor) return;
		if (value) editor.chain().focus().insertContent(`due:${value} `).run();
		else editor.commands.focus();
	}

	const TaskMetaMenu = Extension.create({
		name: 'taskMetaMenu',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('taskMetaMenu'),
					props: {
						handleTextInput: (_view, _from, _to, text) => {
							if (text === '!') taskMetaTypedByUser = true;
							return false;
						},
						handleKeyDown: (_view, event) => {
							if (!taskMetaMenu) return false;
							const n = Math.max(1, taskMetaFiltered.length);
							if (event.key === 'ArrowDown') { event.preventDefault(); taskMetaSelectedIndex = (taskMetaSelectedIndex + 1) % n; return true; }
							if (event.key === 'ArrowUp') { event.preventDefault(); taskMetaSelectedIndex = (taskMetaSelectedIndex - 1 + n) % n; return true; }
							if (event.key === 'Enter' || event.key === 'Tab') {
								if (taskMetaFiltered.length > 0) { event.preventDefault(); selectTaskMeta(taskMetaSelectedIndex); return true; }
								closeTaskMetaMenu();
								return false;
							}
							if (event.key === 'Escape') { event.preventDefault(); closeTaskMetaMenu(); return true; }
							return false;
						},
					},
				}),
			];
		},
	});

	// ── Wiki-links ──

	let wikiLinkMenu = $state<{ x: number; y: number; query: string; from: number } | null>(null);
	let wikiLinkSelectedIndex = $state(0);
	let wikiLinkTitlesCache = $state<NoteTitleEntry[]>([]);
	let wikiLinkTypedByUser = false;
	// Disambiguation state: when ]] auto-close finds multiple matches
	let wikiLinkDisambigEntries = $state<NoteTitleEntry[] | null>(null);
	let wikiLinkDisambigRef = $state<string | null>(null);
	let wikiLinkDisambigDisplay = $state<string | null>(null);
	// Navigation disambiguation: when clicking a wikilink with multiple matches
	let wikiLinkNavDisambig = $state<{ entries: NoteTitleEntry[]; x: number; y: number } | null>(null);
	let wikiLinkNavDisambigIndex = $state(0);
	// Tracks the wiki-link mark under the cursor across transactions, used to detect
	// when the cursor leaves a link so we can rename the linked note if needed.
	let prevCursorWikiMark: any = null;

	let wikiLinkFiltered = $derived.by(() => {
		// When disambiguating, show only the exact matches
		if (wikiLinkDisambigEntries) return wikiLinkDisambigEntries;
		if (!wikiLinkMenu) return wikiLinkTitlesCache;
		let q = wikiLinkMenu.query.toLowerCase();
		if (!q) return wikiLinkTitlesCache;
		// Strip |alias, #heading, ^block - only use the note name part for filtering
		const pipeIdx = q.indexOf('|');
		if (pipeIdx >= 0) q = q.slice(0, pipeIdx);
		q = q.replace(/#.*$/, '').replace(/\^.*$/, '').trim();
		if (!q) return wikiLinkTitlesCache;
		// Score: 0 = exact, 1 = starts-with, 2 = word-start, 3 = contains
		const scored = wikiLinkTitlesCache
			.map(entry => {
				const t = entry.title.toLowerCase();
				let score: number;
				if (t === q) score = 0;
				else if (t.startsWith(q)) score = 1;
				else if (t.includes(' ' + q) || t.includes('-' + q)) score = 2;
				else if (t.includes(q)) score = 3;
				else score = -1;
				return { entry, score };
			})
			.filter(x => x.score >= 0)
			.sort((a, b) => a.score - b.score)
			.map(x => x.entry);
		return scored;
	});

	// Set of lowercase titles that appear more than once (for disambiguation display)
	let wikiLinkDuplicateTitles = $derived.by(() => {
		const counts = new Map<string, number>();
		for (const e of wikiLinkTitlesCache) {
			const key = e.title.toLowerCase();
			counts.set(key, (counts.get(key) ?? 0) + 1);
		}
		const dupes = new Set<string>();
		for (const [key, count] of counts) {
			if (count > 1) dupes.add(key);
		}
		return dupes;
	});

	function wikiLinkFolderPath(entry: NoteTitleEntry): string {
		const vaultRoot = $appConfig?.active_vault;
		if (!vaultRoot || !entry.path) return '';
		// Normalize Windows backslashes so the folder subtitle shows there too.
		const path = entry.path.replace(/\\/g, '/');
		const root = vaultRoot.replace(/\\/g, '/');
		const rel = path.startsWith(root + '/') ? path.slice(root.length + 1) : path;
		const parts = rel.split('/');
		// Return parent folder(s), excluding the filename
		return parts.length > 1 ? parts.slice(0, -1).join('/') + '/' : '';
	}

	async function refreshWikiLinkTitles() {
		try {
			wikiLinkTitlesCache = await getAllNoteTitles();
		} catch (e) {
			console.error('Failed to load note titles:', e);
		}
	}

	function closeWikiLinkMenu() {
		wikiLinkMenu = null;
		wikiLinkSelectedIndex = 0;
		wikiLinkDisambigEntries = null;
		wikiLinkDisambigRef = null;
		wikiLinkDisambigDisplay = null;
	}

	function wikiLinkRelPath(entry: NoteTitleEntry): string | null {
		const vaultRoot = $appConfig?.active_vault;
		if (!vaultRoot || !entry.path || !entry.path.startsWith(vaultRoot + '/')) return null;
		return entry.path.slice(vaultRoot.length + 1).replace(/\.md$/, '');
	}

	// Scan the document for wiki-link marks whose visible text has been changed by
	// the user (text ≠ stored title, and the link is not an explicit alias).
	// Each match triggers a note rename and the mark is updated in-place.
	async function checkWikiLinkRenames() {
		if (!editor || !$appConfig?.enable_wiki_links || isLoadingNote) return;

		type RenameItem = { pos: number; size: number; mark: any; newTitle: string };
		const toRename: RenameItem[] = [];

		editor.state.doc.descendants((node: any, pos: number) => {
			if (!node.isText) return;
			const wikiMark = node.marks.find((m: any) => m.type.name === 'wikiLink');
			// Skip unresolved links (no path) and explicit aliases
			if (!wikiMark || !wikiMark.attrs.path || wikiMark.attrs.aliased) return;
			const text = (node.text as string || '').trim();
			const storedTitle = (wikiMark.attrs.title as string || '').trim();
			if (text && text !== storedTitle) {
				toRename.push({ pos, size: node.nodeSize, mark: wikiMark, newTitle: text });
			}
		});

		if (toRename.length === 0) return;

		// Save the current note before renaming so the Rust backend has the latest content
		await forceSave();

		for (const item of toRename) {
			try {
				const newPath = await renameNote(item.mark.attrs.path, item.newTitle);
				// Update the mark attrs in the editor (title + path) so the next save
				// serialises [[NewTitle]] and doesn't re-trigger the rename check
				if (editor) {
					const wikiMarkType = editor.schema.marks.wikiLink;
					const tr = editor.state.tr;
					tr.addMark(
						item.pos,
						item.pos + item.size,
						wikiMarkType.create({ title: item.newTitle, path: newPath, aliased: false }),
					);
					ignoreNextUpdate = true;
					editor.view.dispatch(tr);
				}
				refreshWikiLinkTitles();
			} catch (e) {
				console.error('Failed to rename note from wiki-link edit:', e);
			}
		}
	}

	function insertWikiLink(entry: NoteTitleEntry, originalRef?: string) {
		if (!editor || !wikiLinkMenu) return;
		const { from } = wikiLinkMenu;
		// Delete the [[ trigger and query text
		const to = editor.state.selection.from;
		editor.chain().focus().deleteRange({ from, to }).run();
		// Insert the wiki-link mark
		// For ambiguous titles, use vault-relative path as the ref so it survives source-mode roundtrips
		const displayText = entry.title;
		let titleAttr = originalRef || entry.title;
		if (entry.path && wikiLinkDuplicateTitles.has(entry.title.toLowerCase())) {
			const relPath = wikiLinkRelPath(entry);
			if (relPath) {
				// Preserve any #heading or ^block anchors from the original ref
				const anchor = originalRef ? originalRef.replace(/^[^#^]*/, '') : '';
				titleAttr = relPath + anchor;
			}
		}
		tick().then(() => {
			if (!editor) return;
			editor.chain().focus()
				.insertContent({
					type: 'text',
					text: displayText,
					marks: [{ type: 'wikiLink', attrs: { title: titleAttr, path: entry.path, aliased: displayText !== titleAttr } }],
				})
				.run();
		});
		closeWikiLinkMenu();
	}

	function executeWikiLinkCommand(index: number) {
		const items = wikiLinkFiltered;
		if (index < 0 || index >= items.length) return;
		if (wikiLinkDisambigEntries) {
			// In disambiguation mode: use stored display/ref
			insertWikiLink({ ...items[index], title: wikiLinkDisambigDisplay || items[index].title }, wikiLinkDisambigRef || undefined);
		} else {
			insertWikiLink(items[index]);
		}
	}

	const WikiLink = TiptapMark.create({
		name: 'wikiLink',
		// inclusive: true so typing at the end of a link extends the mark, allowing
		// the user to edit a title in-place. Moving the cursor one step past the mark
		// exits it, so typing plain text after a link still works normally.
		inclusive: true,
		excludes: 'link',
		addAttributes() {
			return {
				title: { default: null },
				path: { default: null },
				// aliased=true when the display text was explicitly different from the note
				// title (e.g. [[Note|display]]). Aliased links are never used for rename detection.
				aliased: { default: false },
			};
		},
		parseHTML() {
			return [
				{
					tag: 'span[data-wiki-link]',
					getAttrs: (el: HTMLElement) => {
						const title = el.getAttribute('data-title') || null;
						const text = el.textContent || '';
						return {
							title,
							path: el.getAttribute('data-path') || null,
							// Detect alias from the HTML: if the visible text differs from the
							// stored title the link was serialised as [[ref|display]]
							aliased: el.getAttribute('data-aliased') === '1' || (!!title && text !== title),
						};
					},
				},
				{
					tag: 'a[data-wiki-link]',
					getAttrs: (el: HTMLElement) => {
						const title = el.getAttribute('data-title') || null;
						const text = el.textContent || '';
						return {
							title,
							path: el.getAttribute('data-path') || null,
							aliased: el.getAttribute('data-aliased') === '1' || (!!title && text !== title),
						};
					},
				},
			];
		},
		renderHTML({ HTMLAttributes }: { HTMLAttributes: Record<string, any> }) {
			const attrs: Record<string, string> = {
				'data-wiki-link': '',
				'data-path': HTMLAttributes.path || '',
				'data-title': HTMLAttributes.title || '',
				class: 'wiki-link',
			};
			if (HTMLAttributes.aliased) attrs['data-aliased'] = '1';
			return ['span', attrs, 0];
		},
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('wikiLinkClick'),
					props: {
						handleDOMEvents: {
							click: (view, event) => {
								const target = event.target as HTMLElement;
								const wikiLinkEl = target.closest?.('span[data-wiki-link]') as HTMLElement | null;
								if (wikiLinkEl) {
									event.preventDefault();
									event.stopPropagation();
									const path = wikiLinkEl.getAttribute('data-path') || '';
									// Prefer displayed text over stored title — if the user edited the
									// link text after the note was deleted, textContent reflects the
									// new name they want, while data-title still holds the old one.
									const title = wikiLinkEl.textContent || wikiLinkEl.getAttribute('data-title') || '';
									navigateToWikiLink(path, title, event as MouseEvent);
									return true;
								}
								return false;
							},
						},
					},
				}),
			];
		},
	});

	const WikiLinkAutocomplete = Extension.create({
		name: 'wikiLinkAutocomplete',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					key: new PluginKey('wikiLinkAutocomplete'),
					props: {
						handleKeyDown: (_view, event) => {
							if (!wikiLinkMenu) return false;
							if (event.key === 'ArrowDown') {
								event.preventDefault();
								wikiLinkSelectedIndex = (wikiLinkSelectedIndex + 1) % Math.max(1, wikiLinkFiltered.length);
								return true;
							}
							if (event.key === 'ArrowUp') {
								event.preventDefault();
								wikiLinkSelectedIndex = (wikiLinkSelectedIndex - 1 + wikiLinkFiltered.length) % Math.max(1, wikiLinkFiltered.length);
								return true;
							}
							if (event.key === 'Enter' || event.key === 'Tab') {
								if (wikiLinkFiltered.length > 0) {
									event.preventDefault();
									executeWikiLinkCommand(wikiLinkSelectedIndex);
									return true;
								}
								closeWikiLinkMenu();
								return false;
							}
							if (event.key === 'Escape') {
								event.preventDefault();
								closeWikiLinkMenu();
								return true;
							}
							return false;
						},
						handleTextInput: (view, from, to, text) => {
							if (!$appConfig?.enable_wiki_links) return false;
							// Detect [[ opening: flag so onTransaction opens the menu on mobile
							if (text === '[') {
								const charBefore = from > 0 ? view.state.doc.textBetween(from - 1, from) : '';
								if (charBefore === '[') wikiLinkTypedByUser = true;
							}
							// Detect ]] closing: auto-resolve the current text as a wiki-link
							if (text === ']' && wikiLinkMenu) {
								const state = view.state;
								const textBefore = state.doc.textBetween(wikiLinkMenu.from, state.selection.from);
								if (textBefore.endsWith(']')) {
									// Supports Obsidian syntax: [[note|alias]], [[note#heading]], [[note^block]]
									const rawQuery = textBefore.slice(2, -1); // strip the [[ and trailing ]
									if (rawQuery.trim()) {
										const pipeIdx = rawQuery.indexOf('|');
										const noteRef = (pipeIdx >= 0 ? rawQuery.slice(0, pipeIdx) : rawQuery).trim();
										const display = (pipeIdx >= 0 ? rawQuery.slice(pipeIdx + 1) : noteRef).trim();
										// Strip #heading and ^block for title matching
										const titleForLookup = noteRef.replace(/#.*$/, '').replace(/\^.*$/, '').trim();
										const matches = wikiLinkTitlesCache.filter(e => e.title.toLowerCase() === titleForLookup.toLowerCase());
										if (matches.length === 1) {
											insertWikiLink({ ...matches[0], title: display }, noteRef);
										} else if (matches.length > 1) {
											// Keep the menu open but filter to only the matching entries
											wikiLinkMenu = { ...wikiLinkMenu!, query: titleForLookup };
											// Override filtered results to only show exact matches
											wikiLinkDisambigEntries = matches;
											wikiLinkDisambigRef = noteRef;
											wikiLinkDisambigDisplay = display;
											wikiLinkSelectedIndex = 0;
										} else {
										// Insert as unresolved wiki-link (no path)
										const menuFrom = wikiLinkMenu.from;
										const curTo = state.selection.from;
										closeWikiLinkMenu();
										tick().then(() => {
											if (!editor) return;
											// Use a single ProseMirror transaction to replace [[query] with the
											// wiki-link and clear stored marks atomically, preventing the inclusive
											// mark from bleeding into subsequent text. Do NOT call deleteRange first
											// — that would shift positions and make menuFrom/curTo invalid here.
											const { tr, schema } = editor.view.state;
											const wikiLinkMark = schema.marks.wikiLink.create({
												title: noteRef,
												path: '',
												aliased: display !== noteRef,
											});
											const textNode = schema.text(display, [wikiLinkMark]);
											tr.replaceWith(menuFrom, curTo, textNode);
											tr.setSelection(TextSelection.create(tr.doc, menuFrom + display.length));
											tr.setStoredMarks([]);
											editor.view.dispatch(tr);
										});
										}
									} else {
										closeWikiLinkMenu();
									}
									return true;
								}
							}
							return false;
						},
					},
				}),
			];
		},
	});

	function updateWikiLinkMenu() {
		wikiLinkTypedByUser = false;
		if (!editor || !$appConfig?.enable_wiki_links) return;
		const { state } = editor;
		const { selection } = state;
		const resolvedFrom = selection.$from;
		const parentNode = resolvedFrom.parent;
		if (parentNode.type.name !== 'paragraph' && parentNode.type.name !== 'heading') {
			closeWikiLinkMenu();
			return;
		}
		// Build textBefore from the actual ProseMirror node content so positions are accurate
		// (parentNode.textContent flattens images/atoms, causing position miscalculation)
		let textBefore = '';
		const cursorOffset = resolvedFrom.parentOffset;
		parentNode.forEach((child, offset) => {
			if (offset >= cursorOffset) return false;
			if (child.isText) {
				textBefore += child.text!.slice(0, Math.min(child.nodeSize, cursorOffset - offset));
			}
		});
		// Match [[ — also allow exactly one trailing ] so the menu stays open after
		// the first ] of ]] is typed, letting handleTextInput catch the closing ]]
		const match = textBefore.match(/\[\[([^\]]*)\]?$/);
		if (!match) {
			closeWikiLinkMenu();
			return;
		}
		// Refresh titles when the menu first opens so newly created notes are found
		if (!wikiLinkMenu) refreshWikiLinkTitles();
		const query = match[1];
		// Calculate from as cursor position minus the matched text length ("[[query")
		const from = resolvedFrom.pos - match[0].length;
		const coords = editor.view.coordsAtPos(from);
		let x = coords.left;
		if (x + 280 > window.innerWidth) x = window.innerWidth - 290;
		const visibleBottom = window.innerHeight - keyboardHeight;
		const menuHeight = 360;
		let y = coords.bottom + 4;
		if (y + menuHeight > visibleBottom) y = coords.top - menuHeight - 4;
		if (y < 4) y = 4;
		wikiLinkMenu = { x, y, query, from };
		wikiLinkSelectedIndex = 0;
	}

	async function navigateToWikiLink(path: string, title: string, clickEvent?: MouseEvent) {
		// title may contain #heading or ^block anchors - strip for note lookup
		const noteTitle = title.replace(/#.*$/, '').replace(/\^.*$/, '').trim();
		if (!path) {
			// Try path-based resolution first (for disambiguated refs like "folder/note")
			const vaultRoot = $appConfig?.active_vault;
			if (noteTitle.includes('/') && vaultRoot) {
				const fullPath = vaultRoot + '/' + noteTitle + '.md';
				const pathMatch = wikiLinkTitlesCache.find(e => e.path === fullPath);
				if (pathMatch) {
					path = pathMatch.path;
				} else {
					const lastSegment = noteTitle.split('/').pop()!;
					const segMatches = wikiLinkTitlesCache.filter(e => e.title.toLowerCase() === lastSegment.toLowerCase());
					if (segMatches.length === 1) path = segMatches[0].path;
					else if (segMatches.length > 1) {
						let x = clickEvent ? clickEvent.clientX : window.innerWidth / 2 - 140;
						let y = clickEvent ? clickEvent.clientY + 8 : window.innerHeight / 2 - 100;
						if (x + 280 > window.innerWidth) x = window.innerWidth - 290;
						if (y + 200 > window.innerHeight) y = Math.max(4, window.innerHeight - 200);
						wikiLinkNavDisambig = { entries: segMatches, x, y };
						wikiLinkNavDisambigIndex = 0;
						return;
					}
				}
			}
		}
		if (!path) {
			const matches = wikiLinkTitlesCache.filter(e => e.title.toLowerCase() === noteTitle.toLowerCase());
			if (matches.length === 1) {
				path = matches[0].path;
			} else if (matches.length > 1) {
				let x = clickEvent ? clickEvent.clientX : window.innerWidth / 2 - 140;
				let y = clickEvent ? clickEvent.clientY + 8 : window.innerHeight / 2 - 100;
				if (x + 280 > window.innerWidth) x = window.innerWidth - 290;
				if (y + 200 > window.innerHeight) y = Math.max(4, window.innerHeight - 200);
				wikiLinkNavDisambig = { entries: matches, x, y };
				wikiLinkNavDisambigIndex = 0;
				return;
			} else {
				// Create the note (use clean title, not the anchor ref)
				const cleanTitle = noteTitle.includes('/') ? noteTitle.split('/').pop()! : noteTitle;
				const notebookRel = $activeNotePath
					? $activeNotePath.substring(($appConfig?.active_vault?.length ?? 0) + 1).split('/').slice(0, -1).join('/')
					: null;
				try {
					// Save the current note before navigating away
					await forceSave();
					const { createNote } = await import('$lib/api');
					const newNote = await createNote(notebookRel || null, cleanTitle);
					// Refresh titles cache so the new note resolves on future clicks
					refreshWikiLinkTitles();
					// Navigate to the new note
					const content = await readNote(newNote.path);
					$activeNote = { ...content, content: content.content };
					$activeNotePath = newNote.path;
					$editorDirty = false;
				} catch (e) {
					console.error('Failed to create note from wiki-link:', e);
				}
				return;
			}
		}
		try {
			const content = await readNote(path);
			$activeNote = { ...content, content: content.content };
			$activeNotePath = path;
		} catch (e) {
			// Note at path no longer exists (deleted/moved). Refresh cache and
			// retry as unresolved so the user can recreate it from the link.
			await refreshWikiLinkTitles();
			await navigateToWikiLink('', title, clickEvent);
		}
	}

	async function navigateToWikiLinkDirect(entry: NoteTitleEntry) {
		wikiLinkNavDisambig = null;
		try {
			const content = await readNote(entry.path);
			$activeNote = { ...content, content: content.content };
			$activeNotePath = entry.path;
		} catch (e) {
			console.error('Failed to navigate to wiki-link:', e);
		}
	}

	const textColors = [
		{ name: 'Default', value: '' },
		{ name: 'Red', value: '#ef4444' },
		{ name: 'Orange', value: '#f97316' },
		{ name: 'Amber', value: '#f59e0b' },
		{ name: 'Green', value: '#22c55e' },
		{ name: 'Blue', value: '#3b82f6' },
		{ name: 'Purple', value: '#a855f7' },
		{ name: 'Pink', value: '#ec4899' },
	];

	const highlightColors = [
		{ name: 'Yellow', value: 'rgba(250, 230, 100, 0.25)', swatch: '#f5e050' },
		{ name: 'Green', value: 'rgba(100, 210, 130, 0.22)', swatch: '#5cc870' },
		{ name: 'Blue', value: 'rgba(100, 170, 240, 0.22)', swatch: '#6aabf0' },
		{ name: 'Purple', value: 'rgba(180, 130, 240, 0.22)', swatch: '#a878e8' },
		{ name: 'Pink', value: 'rgba(240, 140, 180, 0.22)', swatch: '#e88aaa' },
		{ name: 'Red', value: 'rgba(240, 120, 120, 0.22)', swatch: '#e07070' },
		{ name: 'Orange', value: 'rgba(240, 170, 90, 0.25)', swatch: '#e8a050' },
		{ name: 'Cyan', value: 'rgba(80, 210, 230, 0.22)', swatch: '#50cce0' },
	];

	const cellColors = [
		{ name: 'None', value: '' },
		{ name: 'Light Red', value: '#fde8e8' },
		{ name: 'Light Orange', value: '#fef3e2' },
		{ name: 'Light Yellow', value: '#fef9e7' },
		{ name: 'Light Green', value: '#e6f8e0' },
		{ name: 'Light Blue', value: '#e0f0fe' },
		{ name: 'Light Purple', value: '#f0e6fe' },
		{ name: 'Light Pink', value: '#fde8f0' },
		{ name: 'Light Gray', value: '#f3f4f6' },
		{ name: 'Dark Red', value: '#7f1d1d' },
		{ name: 'Dark Amber', value: '#713f12' },
		{ name: 'Dark Green', value: '#14532d' },
		{ name: 'Dark Blue', value: '#1e3a5f' },
		{ name: 'Dark Purple', value: '#4c1d95' },
		{ name: 'Dark Pink', value: '#831843' },
		{ name: 'Dark Teal', value: '#064e3b' },
		{ name: 'Dark Cyan', value: '#0c4a6e' },
		{ name: 'Slate', value: '#1e293b' },
		{ name: 'Gray', value: '#374151' },
	];

	function resolveImageSrc(src: string): string {
		// Already a proxied, asset, data, or blob URL
		if (src.startsWith('data:') || src.startsWith('asset:') || src.startsWith('blob:') || src.startsWith('imgproxy:') || src.startsWith('http://imgproxy.localhost') || src.startsWith('https://imgproxy.localhost')) {
			return src;
		}
		// Already an asset-localhost URL
		if (src.startsWith('http://asset.localhost') || src.startsWith('https://asset.localhost')) {
			return src;
		}
		// External http/https URLs: proxy through Tauri's imgproxy protocol
		// to bypass WebKitGTK restrictions on loading external resources
		if (src.startsWith('http://') || src.startsWith('https://')) {
			return convertFileSrc(src, 'imgproxy');
		}
		// Decode percent-encoding (%20 → space, etc.) for filesystem resolution
		let decoded = decodeURIComponent(src);
		// Fix multiple leading slashes (from broken saves)
		if (decoded.match(/^\/{2,}/)) {
			decoded = decoded.replace(/^\/{2,}/, '/');
		}
		if (decoded.startsWith('/')) {
			return convertFileSrc(normalizePath(decoded));
		}
		// Paths containing .helixnotes/ are vault-root relative (our own attachments)
		if (decoded.includes('.helixnotes/')) {
			const vaultRoot = $appConfig?.active_vault;
			if (vaultRoot) {
				// Extract from .helixnotes/ onward in case of prefixed subdir paths
				const idx = decoded.indexOf('.helixnotes/');
				return convertFileSrc(`${vaultRoot}/${decoded.substring(idx)}`);
			}
		}
		// Standard markdown: resolve relative paths against the note's directory
		const notePath = $activeNotePath;
		if (notePath) {
			const noteDir = notePath.substring(0, notePath.lastIndexOf('/'));
			return convertFileSrc(normalizePath(`${noteDir}/${decoded}`));
		}
		// Last fallback: vault root
		const vaultRoot = $appConfig?.active_vault;
		if (vaultRoot) {
			return convertFileSrc(normalizePath(`${vaultRoot}/${decoded}`));
		}
		return src;
	}

	const autoSave = debounce(async () => {
		if (get(viewerNote)) return; // never autosave external viewer files
		if (!$activeNote || !$activeNotePath || !$editorDirty) return;
		// Only fix blob images if a paste occurred (avoids full doc scan on every save)
		if (hasPendingBlobs) {
			hasPendingBlobs = false;
			fixingBlobsPromise = fixBlobImages();
		}
		await fixingBlobsPromise;
		try {
			const body = $sourceMode
				? restoreTitleH1(sourceContent)
				: editorToMarkdown();
			// Safety: never save empty/near-empty body over a note that had real content
			const trimmed = body.replace(/^#.*\n?/, '').trim();
			if (!trimmed && $activeNote.content && $activeNote.content.trim().length > 10) {
				console.warn('Auto-save blocked: refusing to overwrite note with empty content');
				return;
			}
			await saveNote($activeNotePath, $activeNote.meta, body);
			$editorDirty = false;
		} catch (e) {
			console.error('Auto-save failed:', e);
		}
	}, isMobile ? 1500 : 500);

	export async function forceSave() {
		if (get(viewerNote)) return; // viewer files are never written back
		if (!$activeNote || !$activeNotePath) return;
		await fixingBlobsPromise;
		try {
			const body = $sourceMode ? restoreTitleH1(sourceContent) : editorToMarkdown();
			const trimmed = body.replace(/^#.*\n?/, '').trim();
			if (!trimmed && $activeNote.content && $activeNote.content.trim().length > 10) {
				console.warn('Force-save blocked: refusing to overwrite note with empty content');
				return;
			}
			await saveNote($activeNotePath, $activeNote.meta, body);
			$editorDirty = false;
		} catch (e) {
			console.error('Save failed:', e);
		}
	}

	// The markdown body the editor would currently save; used to ignore the file-watcher echo of our own save.
	export function getCurrentBody(): string {
		return $sourceMode ? restoreTitleH1(sourceContent) : editorToMarkdown();
	}

	// ── Tag editing (active note) ──
	function toggleTagMenu(e: MouseEvent) {
		if (tagMenu) { tagMenu = null; return; }
		const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
		const width = 240;
		const x = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
		tagMenu = { x, y: rect.bottom + 4 };
	}

	function addActiveNoteTag(tag: string) {
		if (!$activeNote) return;
		const cleaned = tag.trim().toLowerCase();
		if (!cleaned || $activeNote.meta.tags.includes(cleaned)) return;
		$activeNote = { ...$activeNote, meta: { ...$activeNote.meta, tags: [...$activeNote.meta.tags, cleaned] } };
		$editorDirty = true;
		autoSave();
	}

	function removeActiveNoteTag(tag: string) {
		if (!$activeNote) return;
		$activeNote = { ...$activeNote, meta: { ...$activeNote.meta, tags: $activeNote.meta.tags.filter((t) => t !== tag) } };
		$editorDirty = true;
		autoSave();
	}

	// Sync editor editable state when readOnly store changes (from titlebar or editor)
	$effect(() => {
		const ro = $readOnly;
		untrack(() => {
			if (editor) {
				if (ro && $editorDirty) forceSave();
				editor.setEditable(!ro);
			}
		});
	});

	// Belt-and-suspenders: viewer mode is always read-only, regardless of any other path
	// that might toggle setEditable. Re-asserts every time the editor or viewer state changes.
	$effect(() => {
		const v = $viewerNote;
		untrack(() => {
			if (editor && v) editor.setEditable(false);
		});
	});

	function countWords(text: string): number {
		return text.trim() ? text.trim().split(/\s+/).filter(Boolean).length : 0;
	}

	function updateCounts() {
		let text = '';
		if (get(sourceMode)) {
			text = sourceContent
				.replace(/^---[\s\S]*?---\n?/, '')
				.replace(/!\[[^\]]*\]\([^)]*\)/g, '')
				.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
				.replace(/^#{1,6}\s+/gm, '')
				.replace(/[*_~`]+/g, '')
				.replace(/^>\s*/gm, '')
				.replace(/^[-*+]\s+/gm, '');
		} else if (editor) {
			text = editor.state.doc.textContent;
		}
		wordCount = countWords(text);
		charCount = text.replace(/\s/g, '').length;
	}

	const scheduleCounts = debounce(updateCounts, 250);

	async function toggleInfo() {
		if (!showInfo && $activeNote) {
			historyLoading = true;
			try {
				historyVersions = await getNoteVersions($activeNote.meta.id);
			} catch {
				historyVersions = [];
			}
			historyLoading = false;
		}
		showInfo = !showInfo;
	}

	async function toggleHistory() {
		showHistory = !showHistory;
		historyPreview = null;
		historySelected = null;
		if (showHistory && $activeNote) {
			historyLoading = true;
			try {
				historyVersions = await getNoteVersions($activeNote.meta.id);
			} catch (e) {
				console.error('Failed to load versions:', e);
				historyVersions = [];
			}
			historyLoading = false;
		}
	}

	async function previewVersion(v: VersionEntry) {
		if (!$activeNote) return;
		historySelected = v;
		try {
			historyPreview = await getNoteVersionContent($activeNote.meta.id, v.timestamp);
		} catch (e) {
			console.error('Failed to load version:', e);
		}
	}

	async function restoreVersion() {
		if (!$activeNote || !$activeNotePath || !historySelected) return;
		try {
			const raw = historyPreview ?? await getNoteVersionContent($activeNote.meta.id, historySelected.timestamp);
			// The raw content includes frontmatter - parse out the body
			const fmEnd = raw.indexOf('---', 4);
			const body = fmEnd > 0 ? raw.substring(raw.indexOf('\n', fmEnd) + 1) : raw;
			if (editor) {
				editor.commands.setContent(markdownToHtml(body));
			}
			$editorDirty = true;
			autoSave();
			historyPreview = null;
			historySelected = null;
			showHistory = false;
		} catch (e) {
			console.error('Failed to restore version:', e);
		}
	}

	async function handleCreateVersion() {
		if (!$activeNote || !$activeNotePath) return;
		// Save current content first so the snapshot is up to date
		await forceSave();
		try {
			await createVersion($activeNotePath, $activeNote.meta.id);
			// Refresh the version list
			historyVersions = await getNoteVersions($activeNote.meta.id);
		} catch (e) {
			console.error('Failed to create version:', e);
		}
	}

	function formatVersionDate(iso: string): string {
		try {
			const d = new Date(iso);
			const now = new Date();
			const diffMs = now.getTime() - d.getTime();
			const diffMins = Math.floor(diffMs / 60000);
			if (diffMins < 1) return 'Just now';
			if (diffMins < 60) return `${diffMins}m ago`;
			const diffHours = Math.floor(diffMins / 60);
			if (diffHours < 24) return `${diffHours}h ago`;
			const diffDays = Math.floor(diffHours / 24);
			if (diffDays < 7) return `${diffDays}d ago`;
			return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: d.getFullYear() !== now.getFullYear() ? 'numeric' : undefined });
		} catch {
			return iso;
		}
	}

	function formatVersionSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		return `${(bytes / 1024).toFixed(1)} KB`;
	}

	function clearEditorHistory() {
		if (!editor) return;
		// Recreate editor state with same doc/schema/plugins but fresh plugin state (clears undo/redo)
		const { doc, schema, plugins } = editor.state;
		const newState = EditorState.create({ doc, schema, plugins });
		editor.view.updateState(newState);
	}

	export function focusTitle() {
		tick().then(() => {
			if (titleInput) {
				titleInput.focus();
				titleInput.select();
			}
		});
	}

	async function editorNavigateHistory(direction: -1 | 1) {
		const path = navHistory.go(direction);
		if (!path) return;
		flushSave();
		try {
			const content = await readNote(path);
			$activeNote = content;
			$activeNotePath = path;
			$editorDirty = false;
		} catch {}
	}

	/** Flush unsaved editor content to disk (synchronous serialize + fire-and-forget save).
	 *  Call BEFORE updating $activeNote/$activeNotePath stores when switching notes. */
	export function flushSave() {
		if (!$editorDirty || !$activeNote || !$activeNotePath) return;
		try {
			const body = $sourceMode
				? restoreTitleH1(sourceContent)
				: editorToMarkdown();
			const trimmed = body.replace(/^#.*\n?/, '').trim();
			if (trimmed || !$activeNote.content || $activeNote.content.trim().length <= 10) {
				saveNote($activeNotePath, $activeNote.meta, body);
			}
		} catch (e) {
			console.error('Pre-switch save failed:', e);
		}
		$editorDirty = false;
	}

	export function loadNote(path: string, content: string) {
		loadedPath = path;
		lastSourceMode = $sourceMode;
		isLoadingNote = true;
		isLargeDoc = content.length > LARGE_DOC_CHARS;
		// Apply default view mode when switching notes - but new notes always open in edit mode.
		// Viewer mode (external file) always forces read-only.
		const isViewer = !!get(viewerNote);
		const isNewNote = $activeNote?.meta.title === 'Untitled' && !content.replace(/^---[\s\S]*?---\s*/, '').trim();
		const shouldBeReadOnly = isViewer ? true : (isNewNote ? false : ($appConfig?.default_view_mode ?? false));
		$readOnly = shouldBeReadOnly;
		if (editor) editor.setEditable(!shouldBeReadOnly);
		const editorBody = editorElement?.closest('.editor-body') as HTMLElement | null;
		if ($sourceMode) {
			sourceContent = stripTitleH1(content);
			resetSourceHistory(sourceContent);
			if (editorBody) editorBody.scrollTop = 0;
			isLoadingNote = false;
			updateCounts();
		} else if (editorElement && editor) {
			// Editor already exists, just swap content
			const html = markdownToHtml(content);
			ignoreNextUpdate = true;
			editor.commands.setContent(html);
			// Clear undo/redo history so it doesn't bleed across notes
			clearEditorHistory();
			const text = editor.state.doc.textContent;
			wordCount = countWords(text);
			charCount = text.replace(/\s/g, '').length;
			// Reset scroll and cursor after all ProseMirror/Svelte DOM updates settle
			tick().then(() => {
				if (editorBody) editorBody.scrollTop = 0;
				// Explicitly reset ProseMirror selection to start so TipTap's focus()
				// (triggered by checkbox clicks etc.) doesn't scroll to the old note's cursor position.
				if (editor) {
					const tr = editor.state.tr.setSelection(TextSelection.atStart(editor.state.doc));
					// No tr.scrollIntoView() - must not trigger any scroll
					editor.view.dispatch(tr);
				}
				requestAnimationFrame(() => { if (editorBody) editorBody.scrollTop = 0; });
				isLoadingNote = false;
			});
		} else {
		// Editor element not in DOM yet (first note load).
		// Store content and let the $effect on editorElement handle init.
		pendingContent = content;
		isLoadingNote = false;
	}
	if (!isMobile && showOutline) scheduleOutline();
}

	function stripTitleH1(md: string): string {
		const title = $activeNote?.meta.title;
		if (!$appConfig?.hide_title_in_body || !title) {
			titleWasStripped = false;
			strippedTitle = '';
			strippedHeadingPrefix = '';
			return md;
		}
		// Find the first non-empty line
		const lines = md.split('\n');
		for (let i = 0; i < lines.length; i++) {
			const line = lines[i].trim();
			if (line === '') continue;
			// Check if it's a heading (any level) matching the note title
			// Normalize: lowercase, collapse whitespace, strip common separators (- - _)
			const normalize = (s: string) => s.trim().toLowerCase().replace(/[\s\-—_]+/g, ' ');
			const match = line.match(/^(#{1,6})\s+(.+)$/);
			if (match && normalize(match[2]) === normalize(title)) {
				titleWasStripped = true;
				strippedTitle = title.trim();
				strippedHeadingPrefix = match[1]; // preserve original heading level (e.g. "##")
				lines.splice(i, 1);
				// Also remove a trailing blank line after the heading if present
				if (i < lines.length && lines[i].trim() === '') {
					lines.splice(i, 1);
				}
				return lines.join('\n');
			}
			break; // First non-empty line isn't a matching heading, stop
		}
		titleWasStripped = false;
		strippedTitle = '';
		strippedHeadingPrefix = '';
		return md;
	}

	function restoreTitleH1(md: string): string {
		if (!titleWasStripped || !strippedTitle) return md;
		return `${strippedHeadingPrefix} ${strippedTitle}\n\n${md}`;
	}

	function editorToMarkdown(): string {
		if (!editor) return '';
		const md = prosemirrorToMarkdown(editor.state.doc);
		return restoreTitleH1(md);
	}

	function isImageNode(node: any): boolean {
		if (node.type.name === 'image') return true;
		if (node.type.name !== 'paragraph') return false;
		let hasImage = false;
		let hasOther = false;
		node.forEach((child: any) => {
			if (child.type.name === 'image') hasImage = true;
			else hasOther = true;
		});
		return hasImage && !hasOther;
	}

	function prosemirrorToMarkdown(doc: any): string {
		const entries: { text: string; isImage: boolean }[] = [];
		doc.forEach((node: any) => {
			const isEmpty = node.type.name === 'paragraph' && node.childCount === 0;
			// Preserve every empty paragraph as <!-- --> so markdown round-trip keeps the user's
			// vertical spacing exactly. markdownToHtml converts the marker back to <p></p> on load.
			if (isEmpty) {
				entries.push({ text: '<!-- -->', isImage: false });
				return;
			}
			entries.push({ text: serializeNode(node), isImage: isImageNode(node) });
		});
		// Join: skip extra \n separator before image nodes so they don't get unwanted blank lines
		let result = '';
		for (let i = 0; i < entries.length; i++) {
			if (i === 0) {
				result = entries[i].text;
			} else {
				const separator = entries[i].isImage ? '' : '\n';
				result += separator + entries[i].text;
			}
		}
		return result.replace(/\n{3,}/g, '\n\n').trim() + '\n';
	}

	function tableToMarkdown(table: any): string {
		const rows: string[][] = [];
		let hasHeader = false;
		table.forEach((row: any) => {
			if (row.type.name !== 'tableRow') return;
			const cells: string[] = [];
			row.forEach((cell: any) => {
				if (cell.type.name === 'tableHeader') hasHeader = true;
				const cellText: string[] = [];
				cell.forEach((p: any) => {
					cellText.push(serializeInline(p));
				});
				cells.push(cellText.join(' ').replace(/\|/g, '\\|').replace(/\n/g, ' '));
			});
			rows.push(cells);
		});
		if (rows.length === 0) return '';
		const colCount = Math.max(...rows.map(r => r.length));
		const lines: string[] = [];
		rows.forEach((row, i) => {
			lines.push('| ' + row.join(' | ') + ' |');
			if (i === 0 && hasHeader) {
				lines.push('| ' + Array(colCount).fill('---').join(' | ') + ' |');
			}
		});
		return lines.join('\n');
	}

	function resetTableToMarkdown() {
		if (!editor) return;
		const { state } = editor;
		let { tr } = state;
		let changed = false;
		state.doc.descendants((node: any, pos: number) => {
			if (node.type.name === 'tableCell' || node.type.name === 'tableHeader') {
				if (node.attrs.backgroundColor || (node.attrs.colspan && node.attrs.colspan > 1) || (node.attrs.rowspan && node.attrs.rowspan > 1)) {
					tr = tr.setNodeMarkup(pos, undefined, {
						...node.attrs,
						backgroundColor: null,
						colspan: 1,
						rowspan: 1,
					});
					changed = true;
				}
			}
			return true;
		});
		if (changed) {
			editor.view.dispatch(tr);
			$editorDirty = true;
			autoSave();
		}
		closeTableContextMenu();
	}

	function serializeNode(node: any): string {
		switch (node.type.name) {
			case 'paragraph': {
				const align = node.attrs.textAlign;
				if (align && align !== 'left') {
					return `<p style="text-align: ${align}">${serializeInline(node)}</p>\n`;
				}
				return serializeInline(node) + '\n';
			}
			case 'heading': {
				const align = node.attrs.textAlign;
				if (align && align !== 'left') {
					return `<h${node.attrs.level} style="text-align: ${align}">${serializeInline(node)}</h${node.attrs.level}>\n`;
				}
				return '#'.repeat(node.attrs.level) + ' ' + serializeInline(node) + '\n';
			}
			case 'codeBlock': {
				const lang = node.attrs.language || '';
				const code = node.textContent.replace(/\n+$/, '');
				return '```' + lang + '\n' + code + '\n```\n';
			}
			case 'blockquote': {
				const blocks: string[] = [];
				node.forEach((child: any) => {
					const lines = serializeNode(child).replace(/\n$/, '').split('\n');
					blocks.push(lines.map((l: string) => '> ' + l).join('\n'));
				});
				return blocks.join('\n>\n') + '\n';
			}
			case 'callout':
				return serializeCallout(node, serializeNode);
			case 'bulletList': {
				const items: string[] = [];
				node.forEach((child: any) => items.push('- ' + serializeListItem(child)));
				return items.join('') + '\n';
			}
			case 'orderedList': {
				const items: string[] = [];
				let i = node.attrs.start || 1;
				node.forEach((child: any) => { items.push(`${i++}. ` + serializeListItem(child)); });
				return items.join('') + '\n';
			}
			case 'taskList': {
				const items: string[] = [];
				node.forEach((child: any) => {
					const checked = child.attrs.checked ? 'x' : ' ';
					items.push(`- [${checked}] ` + serializeListItem(child));
				});
				return items.join('') + '\n';
			}
			case 'listItem':
			case 'taskItem':
				return serializeListItem(node);
			case 'horizontalRule':
				return '---\n';
			case 'pageBreak':
				return '<div style="page-break-after: always;"></div>\n';
			case 'table': {
				// Check if any cell has styling (background color) or if there are merged cells
				let hasStyling = false;
				node.descendants((child: any) => {
					if (child.type.name === 'tableCell' || child.type.name === 'tableHeader') {
						if (child.attrs.backgroundColor) hasStyling = true;
						if (child.attrs.colspan && child.attrs.colspan > 1) hasStyling = true;
						if (child.attrs.rowspan && child.attrs.rowspan > 1) hasStyling = true;
					}
					return true;
				});
				if (hasStyling) {
					const tempDiv = document.createElement('div');
					const frag = DOMSerializer.fromSchema(editor!.schema).serializeNode(node);
					tempDiv.appendChild(frag);
					return tempDiv.innerHTML + '\n';
				}
				return tableToMarkdown(node) + '\n';
			}
			case 'pdfEmbed': {
				const src = node.attrs.src || '';
				const name = node.attrs.name || '';
				return `<div data-pdf-src="${src}" data-pdf-name="${name}" class="pdf-embed"></div>\n`;
			}
			case 'secretBlock': {
				const payload = (node.attrs.payload || '').trim();
				return `\`\`\`helix-secret\n${payload}\n\`\`\`\n`;
			}
			case 'mathBlock': {
				const tex = node.attrs.tex || '';
				return `$$\n${tex}\n$$\n`;
			}
			case 'details': {
				// Preserve details as raw HTML
				const detDiv = document.createElement('div');
				const detFrag = DOMSerializer.fromSchema(editor!.schema).serializeNode(node);
				detDiv.appendChild(detFrag);
				return detDiv.innerHTML + '\n';
			}
			case 'image': {
				const src = stripAssetSrc(node.attrs.src || '');
				if (!src) return ''; // Skip images with unresolved blob: URLs
				const alt = node.attrs.alt || '';
				const size = node.attrs['data-size'] || node.attrs.size || 'full';
				const sizeSuffix = size && size !== 'full' ? `|size=${size}` : '';
				return `![${alt}${sizeSuffix}](${src})\n`;
			}
			default:
				return node.textContent || '';
		}
	}

	function serializeListItem(node: any): string {
		const parts: string[] = [];
		node.forEach((child: any) => {
			if (child.type.name === 'paragraph') {
				parts.push(serializeInline(child));
			} else if (child.type.name === 'bulletList' || child.type.name === 'orderedList' || child.type.name === 'taskList') {
				// Indent nested lists so markdown parsers recognize nesting
				// Use 4 spaces - works for both bullet (- ) and ordered (1. ) parent markers
				const nested = serializeNode(child).replace(/\n$/, '');
				const indented = nested.split('\n').map((line: string) => '    ' + line).join('\n');
				parts.push(indented);
			} else {
				parts.push(serializeNode(child));
			}
		});
		return parts.join('\n') + '\n';
	}

	function serializeInline(node: any): string {
		if (node.childCount === 0) return '';
		const parts: string[] = [];
		node.forEach((child: any, _offset: number, index: number) => {
			if (child.isText) {
				let text = child.text || '';
				// Preserve leading tabs/em-spaces as HTML entities so they survive markdown roundtrip
				// (markdown parsers strip tab whitespace, but &emsp; passes through as HTML)
				// Tabs come from initial indent; em-spaces (U+2003) come from prior &emsp; roundtrips
				if (index === 0) {
					text = text.replace(/^[\t\u2003]+/, (ws) => '&emsp;'.repeat(ws.length));
				}
				// Apply marks
				for (const mark of child.marks) {
					switch (mark.type.name) {
						case 'bold': text = `**${text}**`; break;
						case 'italic': text = `*${text}*`; break;
						case 'strike': text = `~~${text}~~`; break;
						case 'code': text = `\`${text}\``; break;
						case 'underline': text = `<u>${text}</u>`; break;
						case 'subscript': text = `~${text}~`; break;
						case 'superscript': text = `^${text}^`; break;
						case 'highlight': {
							const color = mark.attrs?.color;
							if (color) {
								text = `<mark data-color="${color}">${text}</mark>`;
							} else {
								text = `==${text}==`;
							}
							break;
						}
						case 'textStyle': {
							const c = mark.attrs?.color;
							if (c) text = `<span style="color: ${c}">${text}</span>`;
							break;
						}
						case 'link': text = `[${text}](${mark.attrs.href})`; break;
						case 'wikiLink': {
							const wlTitle = mark.attrs.title || text;
							// If display text differs from the reference, emit [[ref|display]] (Obsidian alias syntax)
							text = wlTitle !== text ? `[[${wlTitle}|${text}]]` : `[[${wlTitle}]]`;
							break;
						}
					}
				}
				parts.push(text);
			} else if (child.type.name === 'image') {
				const src = stripAssetSrc(child.attrs.src || '');
				if (!src) return; // Skip images with unresolved blob: URLs
				const alt = child.attrs.alt || '';
				const size = child.attrs['data-size'] || child.attrs.size || 'full';
				const sizeSuffix = size && size !== 'full' ? `|size=${size}` : '';
				if (parts.length > 0 && parts[parts.length - 1] !== '\n') {
					parts.push('\n');
				}
				parts.push(`![${alt}${sizeSuffix}](${src})`);
			} else if (child.type.name === 'mathInline') {
				parts.push(`$${child.attrs.tex || ''}$`);
			} else if (child.type.name === 'hardBreak') {
				parts.push('  \n');
			}
		});
		return parts.join('');
	}

	function autofocus(el: HTMLElement) {
		requestAnimationFrame(() => el.focus());
	}

	// ── In-note search functions ──
	let noteSearchTimer: ReturnType<typeof setTimeout> | null = null;

	function updateNoteSearch(query: string) {
		if (noteSearchTimer) clearTimeout(noteSearchTimer);
		if (!query) {
			noteSearchResults = [];
			noteSearchIndex = 0;
			if (!$sourceMode && editor) {
				const tr = editor.state.tr.setMeta(noteSearchPluginKey, DecorationSet.empty);
				editor.view.dispatch(tr);
			}
			return;
		}
		noteSearchTimer = setTimeout(() => {
			if ($sourceMode) {
				updateNoteSearchSource(query);
			} else {
				updateNoteSearchWysiwyg(query);
			}
		}, 100);
	}

	function updateNoteSearchWysiwyg(query: string) {
		if (!editor) return;
		const results: {from: number, to: number}[] = [];
		const lowerQuery = query.toLowerCase();
		editor.state.doc.descendants((node, pos) => {
			if (!node.isText || !node.text) return;
			const text = node.text.toLowerCase();
			let idx = text.indexOf(lowerQuery);
			while (idx !== -1) {
				results.push({ from: pos + idx, to: pos + idx + query.length });
				idx = text.indexOf(lowerQuery, idx + 1);
			}
		});
		noteSearchResults = results;
		if (noteSearchIndex >= results.length) noteSearchIndex = 0;
		applySearchDecorations();
	}

	function updateNoteSearchSource(query: string) {
		const results: {from: number, to: number}[] = [];
		const lowerQuery = query.toLowerCase();
		const text = sourceContent.toLowerCase();
		let idx = text.indexOf(lowerQuery);
		while (idx !== -1) {
			results.push({ from: idx, to: idx + query.length });
			idx = text.indexOf(lowerQuery, idx + 1);
		}
		noteSearchResults = results;
		if (noteSearchIndex >= results.length) noteSearchIndex = 0;
		scrollToSourceMatch();
	}

	function scrollToSourceMatch(focusTextarea = false) {
		if (!sourceElement || noteSearchResults.length === 0) return;
		const match = noteSearchResults[noteSearchIndex];
		// Only steal focus when navigating (Enter/Shift+Enter), not while typing
		if (focusTextarea) sourceElement.focus();
		sourceElement.setSelectionRange(match.from, match.to);
		scrollSourceToOffset(match.from);
	}

	// Centre the source textarea's viewport on the line containing `offset`.
	function scrollSourceToOffset(offset: number) {
		if (!sourceElement) return;
		const linesBefore = sourceContent.substring(0, offset).split('\n').length;
		const lineHeight = parseFloat(getComputedStyle(sourceElement).lineHeight) || 20;
		sourceElement.scrollTop = Math.max(0, (linesBefore - 1) * lineHeight - sourceElement.clientHeight / 2);
	}

	// ── Caret carry-over between the rich editor and the markdown source view (issue #125) ──
	// The caret is a ProseMirror doc position in one mode and a character offset in the other,
	// and the markdown<->doc conversion is lossy, so rather than invert the conversion we anchor
	// on VISIBLE TEXT. `scanAlign` walks the markdown source against the doc's plain text (its
	// non-whitespace characters); markup characters don't match the next expected visible char and
	// are skipped, so the scan locks onto real words and self-resynchronises. This stays correct
	// even at the end of a long note and is decoupled from how serialization happens to format.
	//   - stopAtNw >= 0: stop once that many visible chars have matched -> srcOffset is the source
	//     position just past them (maps a doc caret to a source offset).
	//   - limit >= 0: stop at source index `limit` -> nwCount is how many visible chars precede it
	//     (maps a source caret to a doc visible-char count).
	function scanAlign(source: string, docNonWs: string, opts: { limit?: number; stopAtNw?: number }): { srcOffset: number; nwCount: number } {
		const limit = opts.limit ?? -1;
		const stopAtNw = opts.stopAtNw ?? -1;
		if (stopAtNw === 0) return { srcOffset: 0, nwCount: 0 };
		let k = 0;
		let i = 0;
		for (; i < source.length; i++) {
			if (limit >= 0 && i >= limit) break;
			const c = source[i];
			if (c === ' ' || c === '\n' || c === '\t' || c === '\r') continue;
			if (c === docNonWs[k]) {
				k++;
				if (stopAtNw >= 0 && k >= stopAtNw) { i++; break; }
			}
		}
		return { srcOffset: i, nwCount: k };
	}

	// The whole editor doc as plain text with whitespace removed (the alignment alphabet).
	function docNonWhitespace(): string {
		if (!editor) return '';
		return editor.state.doc.textBetween(0, editor.state.doc.content.size, '\n', '').replace(/\s/g, '');
	}

	// Editor doc position just after the `targetNw`-th non-whitespace character.
	function docPosForNonWsCount(doc: any, targetNw: number): number {
		if (targetNw <= 0) return 0;
		const nwAt = (pos: number) => doc.textBetween(0, pos, '\n', '').replace(/\s/g, '').length;
		let lo = 0;
		let hi = doc.content.size;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (nwAt(mid) >= targetNw) hi = mid; else lo = mid + 1;
		}
		return lo;
	}

	function resetSourceHistory(content: string) {
		sourceHistory = [{ content, cursor: content.length }];
		sourceHistoryIndex = 0;
		if (sourceHistoryTimer) {
			clearTimeout(sourceHistoryTimer);
			sourceHistoryTimer = null;
		}
	}

	function pushSourceHistoryImmediate() {
		if (!sourceElement) return;
		if (sourceHistoryTimer) {
			clearTimeout(sourceHistoryTimer);
			sourceHistoryTimer = null;
		}
		const entry = { content: sourceContent, cursor: sourceElement.selectionStart };
		// Don't push duplicate
		if (sourceHistoryIndex >= 0 && sourceHistory[sourceHistoryIndex]?.content === entry.content) return;
		// Truncate any redo history
		sourceHistory = sourceHistory.slice(0, sourceHistoryIndex + 1);
		sourceHistory.push(entry);
		sourceHistoryIndex++;
		// Limit stack size
		if (sourceHistory.length > 200) {
			sourceHistory.shift();
			sourceHistoryIndex--;
		}
	}

	function pushSourceHistoryDebounced() {
		if (sourceHistoryTimer) clearTimeout(sourceHistoryTimer);
		sourceHistoryTimer = setTimeout(() => {
			sourceHistoryTimer = null;
			pushSourceHistoryImmediate();
		}, 300);
	}

	function sourceUndo() {
		// Flush any pending debounced snapshot first
		if (sourceHistoryTimer) {
			clearTimeout(sourceHistoryTimer);
			sourceHistoryTimer = null;
			pushSourceHistoryImmediate();
		}
		if (sourceHistoryIndex <= 0) return;
		sourceHistoryIndex--;
		const entry = sourceHistory[sourceHistoryIndex];
		sourceContent = entry.content;
		tick().then(() => {
			sourceElement?.setSelectionRange(entry.cursor, entry.cursor);
		});
	}

	function sourceRedo() {
		if (sourceHistoryIndex >= sourceHistory.length - 1) return;
		sourceHistoryIndex++;
		const entry = sourceHistory[sourceHistoryIndex];
		sourceContent = entry.content;
		tick().then(() => {
			sourceElement?.setSelectionRange(entry.cursor, entry.cursor);
		});
	}

	function applySearchDecorations() {
		if (!editor) return;
		const decorations = noteSearchResults.map((m, i) =>
			Decoration.inline(m.from, m.to, { class: i === noteSearchIndex ? 'note-search-match note-search-active' : 'note-search-match' })
		);
		const decoSet = DecorationSet.create(editor.state.doc, decorations);
		const tr = editor.state.tr.setMeta(noteSearchPluginKey, decoSet);
		editor.view.dispatch(tr);
		scrollToCurrentMatch();
	}

	function scrollToCurrentMatch() {
		if (!editor || noteSearchResults.length === 0) return;
		requestAnimationFrame(() => {
			const el = editor?.view.dom.querySelector('.note-search-active');
			if (el) {
				el.scrollIntoView({ block: 'center', behavior: 'smooth' });
			}
		});
	}

	function noteSearchNext() {
		if (noteSearchResults.length === 0) return;
		noteSearchIndex = (noteSearchIndex + 1) % noteSearchResults.length;
		if ($sourceMode) {
			scrollToSourceMatch(true);
		} else {
			applySearchDecorations();
		}
	}

	function noteSearchPrev() {
		if (noteSearchResults.length === 0) return;
		noteSearchIndex = (noteSearchIndex - 1 + noteSearchResults.length) % noteSearchResults.length;
		if ($sourceMode) {
			scrollToSourceMatch(true);
		} else {
			applySearchDecorations();
		}
	}

	export function openNoteSearch() {
		noteSearchOpen = true;
	}

	function closeNoteSearch() {
		noteSearchOpen = false;
		noteSearchQuery = '';
		noteSearchResults = [];
		noteSearchIndex = 0;
		if (!$sourceMode && editor) {
			const tr = editor.state.tr.setMeta(noteSearchPluginKey, DecorationSet.empty);
			editor.view.dispatch(tr);
			editor.commands.focus();
		}
	}

	function stripAssetSrc(src: string): string {
		// blob: URLs are not persistable - they were temporary browser references
		if (src.startsWith('blob:')) return '';
		// Convert imgproxy:// URLs back to original external URLs for saving
		if (src.startsWith('imgproxy:') || src.startsWith('http://imgproxy.localhost') || src.startsWith('https://imgproxy.localhost')) {
			try {
				const url = new URL(src);
				return decodeURIComponent(url.pathname.substring(1));
			} catch {
				return src;
			}
		}
		// Convert asset:// URLs back to relative paths for saving
		if (!src.startsWith('asset:') && !src.startsWith('http://asset.localhost')) return src;
		let absPath = '';
		try {
			const url = new URL(src);
			absPath = decodeURIComponent(url.pathname);
		} catch {
			return src;
		}
		// Clean up any leading double/triple slashes (URL parsing artifact)
		absPath = absPath.replace(/^\/{2,}/, '/');
		// Make relative to note directory (matches how resolveImageSrc works)
		const notePath = $activeNotePath;
		if (notePath) {
			const noteDir = notePath.substring(0, notePath.lastIndexOf('/'));
			if (absPath.startsWith(noteDir + '/')) {
				return absPath.substring(noteDir.length + 1);
			}
		}
		// Fallback: make relative to vault root
		const vaultRoot = $appConfig?.active_vault;
		if (vaultRoot && absPath.startsWith(vaultRoot + '/')) {
			return absPath.substring(vaultRoot.length + 1);
		}
		return absPath;
	}

	function htmlToMarkdown(html: string): string {
		let md = html;
		// Code blocks MUST be converted before inline code to avoid corruption
		md = md.replace(/<pre><code[^>]*class="language-(\w+)"[^>]*>([\s\S]*?)<\/code><\/pre>/gi, (_, lang, code) => {
			const stripped = code.replace(/<[^>]+>/g, '');
			const decoded = stripped.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
			return '```' + lang + '\n' + decoded + '\n```\n';
		});
		md = md.replace(/<pre><code[^>]*>([\s\S]*?)<\/code><\/pre>/gi, (_, code) => {
			const stripped = code.replace(/<[^>]+>/g, '');
			const decoded = stripped.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
			return '```\n' + decoded + '\n```\n';
		});
		md = md.replace(/<h1[^>]*>(.*?)<\/h1>/gi, '# $1\n');
		md = md.replace(/<h2[^>]*>(.*?)<\/h2>/gi, '## $1\n');
		md = md.replace(/<h3[^>]*>(.*?)<\/h3>/gi, '### $1\n');
		md = md.replace(/<h4[^>]*>(.*?)<\/h4>/gi, '#### $1\n');
		md = md.replace(/<h5[^>]*>(.*?)<\/h5>/gi, '##### $1\n');
		md = md.replace(/<h6[^>]*>(.*?)<\/h6>/gi, '###### $1\n');
		md = md.replace(/<strong>(.*?)<\/strong>/gi, '**$1**');
		md = md.replace(/<b>(.*?)<\/b>/gi, '**$1**');
		md = md.replace(/<em>(.*?)<\/em>/gi, '*$1*');
		md = md.replace(/<i>(.*?)<\/i>/gi, '*$1*');
		md = md.replace(/<s>(.*?)<\/s>/gi, '~~$1~~');
		md = md.replace(/<del>(.*?)<\/del>/gi, '~~$1~~');
		md = md.replace(/<u>(.*?)<\/u>/gi, '<u>$1</u>');
		md = md.replace(/<sub>(.*?)<\/sub>/gi, '~$1~');
		md = md.replace(/<sup>(.*?)<\/sup>/gi, '^$1^');
		md = md.replace(/<code>(.*?)<\/code>/gi, '`$1`');
		md = md.replace(/<mark data-color="([^"]*)">(.*?)<\/mark>/gi, '<mark data-color="$1">$2</mark>');
		md = md.replace(/<mark>(.*?)<\/mark>/gi, '==$1==');
		md = md.replace(/<blockquote[^>]*>([\s\S]*?)<\/blockquote>/gi, (_, content) => {
			return content
				.replace(/<p[^>]*>(.*?)<\/p>/gi, '> $1\n')
				.replace(/<br\s*\/?>/gi, '\n> ');
		});
		md = md.replace(/<a[^>]*href="([^"]*)"[^>]*>(.*?)<\/a>/gi, (_m, href, text) => {
			// Decode percent-encoded href back to readable form for markdown source
			// Spaces are re-encoded by markdownToHtml preprocessing before markdown-it parsing
			const decoded = decodeURIComponent(href);
			return `[${text}](${decoded})`;
		});
		md = md.replace(/<img[^>]*>/gi, (match) => {
			const srcMatch = match.match(/src="([^"]*)"/);
			const altMatch = match.match(/alt="([^"]*)"/);
			const sizeMatch = match.match(/data-size="([^"]*)"/);
			const src = srcMatch ? stripAssetSrc(srcMatch[1]) : '';
			const alt = altMatch ? altMatch[1] : '';
			const size = sizeMatch ? sizeMatch[1] : 'full';
			const sizeSuffix = size && size !== 'full' ? `|size=${size}` : '';
			return `![${alt}${sizeSuffix}](${src})`;
		});
		// Preserve PDF embeds as raw HTML
		const pdfs: string[] = [];
		md = md.replace(/<div[^>]*data-pdf-src="([^"]*)"[^>]*>[\s\S]*?<\/div>/gi, (match, src) => {
			// Store with the relative path - we strip convertFileSrc URLs on save
			const nameMatch = match.match(/data-pdf-name="([^"]*)"/);
			const name = nameMatch ? nameMatch[1] : src.split('/').pop() || 'file.pdf';
			pdfs.push(`<div data-pdf-src="${src}" data-pdf-name="${name}" class="pdf-embed"></div>`);
			return `\n%%PDF_${pdfs.length - 1}%%\n`;
		});
		// Preserve details/accordion blocks as raw HTML
		const detailsBlocks: string[] = [];
		md = md.replace(/<details[\s\S]*?<\/details>/gi, (match) => {
			detailsBlocks.push(match);
			return `\n%%DETAILS_${detailsBlocks.length - 1}%%\n`;
		});
		// Preserve tables as raw HTML (markdown tables are too limited)
		const tables: string[] = [];
		md = md.replace(/<table[\s\S]*?<\/table>/gi, (match) => {
			tables.push(match);
			return `\n%%TABLE_${tables.length - 1}%%\n`;
		});

		md = md.replace(/<hr\s*\/?>/gi, '---\n');
		md = md.replace(/<ul[^>]*>([\s\S]*?)<\/ul>/gi, (_, content) => {
			return content.replace(/<li[^>]*><p[^>]*>(.*?)<\/p><\/li>/gi, '- $1\n')
				.replace(/<li[^>]*>(.*?)<\/li>/gi, '- $1\n');
		});
		md = md.replace(/<ol[^>]*>([\s\S]*?)<\/ol>/gi, (_, content) => {
			let i = 0;
			return content.replace(/<li[^>]*><p[^>]*>(.*?)<\/p><\/li>/gi, () => `${++i}. $1\n`)
				.replace(/<li[^>]*>(.*?)<\/li>/gi, () => `${++i}. $1\n`);
		});
		md = md.replace(/<li[^>]*data-checked="true"[^>]*>(.*?)<\/li>/gi, '- [x] $1\n');
		md = md.replace(/<li[^>]*data-checked="false"[^>]*>(.*?)<\/li>/gi, '- [ ] $1\n');
		md = md.replace(/<p[^>]*>(.*?)<\/p>/gi, '$1\n\n');
		md = md.replace(/<br\s*\/?>/gi, '\n');
		md = md.replace(/<[^>]+>/g, '');
		md = md.replace(/&amp;/g, '&');
		md = md.replace(/&lt;/g, '<');
		md = md.replace(/&gt;/g, '>');
		md = md.replace(/&quot;/g, '"');
		md = md.replace(/&#39;/g, "'");
		md = md.replace(/\n{3,}/g, '\n\n');
		// Restore table HTML
		tables.forEach((table, i) => {
			md = md.replace(`%%TABLE_${i}%%`, '\n' + table + '\n');
		});
		// Restore PDF embeds
		pdfs.forEach((pdf, i) => {
			md = md.replace(`%%PDF_${i}%%`, '\n' + pdf + '\n');
		});
		// Restore details/accordion blocks
		detailsBlocks.forEach((block, i) => {
			md = md.replace(`%%DETAILS_${i}%%`, '\n' + block + '\n');
		});
		return md.trim() + '\n';
	}

	function secretFencesToHtml(md: string): string {
		const lines = md.split('\n');
		const out: string[] = [];
		for (let i = 0; i < lines.length; i++) {
			if (!/^\s*```helix-secret\s*$/.test(lines[i])) {
				out.push(lines[i]);
				continue;
			}

			const payloadLines: string[] = [];
			let closedAt = -1;
			for (let j = i + 1; j < lines.length; j++) {
				if (/^\s*```\s*$/.test(lines[j])) {
					closedAt = j;
					break;
				}
				payloadLines.push(lines[j]);
			}

			if (closedAt === -1) {
				out.push(lines[i], ...payloadLines);
				break;
			}

			const payload = payloadLines.join('\n');
			out.push(`<div data-secret-block="${escapeHtml(encodeURIComponent(payload))}"></div>`);
			i = closedAt;
		}
		return out.join('\n');
	}

	function markdownToHtml(md: string): string {
		let src = stripTitleH1(md);
		src = secretFencesToHtml(src);

		// Pre-process: convert [[Note Title]] wiki-links to HTML anchors
		// Supports Obsidian syntax: [[note|alias]], [[note#heading]], [[note^block]]
		if ($appConfig?.enable_wiki_links) {
			src = src.replace(/\[\[([^\]]+)\]\]/g, (_, raw) => {
				// Split on pipe: [[note|display text]] → noteRef="note", display="display text"
				const pipeIdx = raw.indexOf('|');
				const noteRef = (pipeIdx >= 0 ? raw.slice(0, pipeIdx) : raw).trim();
				const display = (pipeIdx >= 0 ? raw.slice(pipeIdx + 1) : noteRef).trim();
				// Strip #heading and ^block anchors for title matching
				const titleForLookup = noteRef.replace(/#.*$/, '').replace(/\^.*$/, '').trim();
				// Try to resolve: first by vault-relative path (for disambiguated links), then by title
				const vaultRoot = $appConfig?.active_vault ?? '';
				let match: NoteTitleEntry | undefined;
				if (titleForLookup.includes('/') && vaultRoot) {
					const fullPath = vaultRoot + '/' + titleForLookup + '.md';
					match = wikiLinkTitlesCache.find(e => e.path === fullPath);
				}
				if (!match) {
					// Fallback: resolve by title (use the last segment if path-based)
					const titleOnly = titleForLookup.includes('/') ? titleForLookup.split('/').pop()! : titleForLookup;
					const titleLower = titleOnly.toLowerCase();
					const titleMatches = wikiLinkTitlesCache.filter(e => e.title.toLowerCase() === titleLower);
					if (titleMatches.length === 1) {
						match = titleMatches[0];
					} else if (titleMatches.length > 1) {
						// Multiple matches - prefer the shallowest path (closest to vault root)
						match = titleMatches.reduce((a, b) =>
							a.path.split('/').length <= b.path.split('/').length ? a : b
						);
					}
				}
				const path = match ? match.path : '';
				return `<span data-wiki-link data-path="${escapeHtml(path)}" data-title="${escapeHtml(noteRef)}" class="wiki-link">${escapeHtml(display)}</span>`;
			});
		}

		// Pre-process: fix image paths with multiple leading slashes (from broken saves)
		src = src.replace(/!\[([^\]]*)\]\((\/{2,})(home\/)/g, '![$1](/home/');

		// Pre-process: normalize link/image destinations for strict-CommonMark markdown-it.
		// Handles angle-bracket destinations <url>, optional titles ("..."/'...'/(...)), and the
		// non-standard %20 separator some exporters (e.g. Notesnook) emit between url and title.
		// Emits clean [label](encoded-url); titles are dropped (we never serialize them anyway).
		src = src.replace(
			/(!?)(\[[^\]]*\])\(<([^>]*)>(?:(?:\s|%20)*(?:"[^"]*"|'[^']*'|\([^)]*\)))?(?:\s|%20)*\)/g,
			(_m, bang, label, url) => `${bang}${label}(${url.replace(/ /g, '%20')})`
		);
		// Bare destination with a trailing quoted title: [label](url "title") -> drop title, encode spaces.
		// Stops our own space-encoder below from mangling valid CommonMark titled links.
		src = src.replace(
			/(!?)(\[[^\]]*\])\(([^()<>]*?)(?:\s|%20)+(?:"[^"]*"|'[^']*')(?:\s|%20)*\)/g,
			(_m, bang, label, url) => `${bang}${label}(${url.replace(/ /g, '%20')})`
		);

		// Pre-process: percent-encode spaces in image URLs so markdown-it parses them correctly
		src = src.replace(/!\[([^\]]*)\]\(([^)]*\s[^)]*)\)/g, (match, alt, url) => {
			return `![${alt}](${url.replace(/ /g, '%20')})`;
		});

		// Pre-process: percent-encode spaces in link URLs so markdown-it parses them correctly
		// Matches [text](url with spaces) but not ![image](url) (already handled above)
		src = src.replace(/(?<!!)\[([^\]]*)\]\(([^)]*\s[^)]*)\)/g, (_match, text, url) => {
			return `[${text}](${url.replace(/ /g, '%20')})`;
		});

		// Pre-process: transform PDF embed divs - iframes when inline preview is on, clickable links otherwise
		src = src.replace(/<div[^>]*data-pdf-src="([^"]*)"[^>]*data-pdf-name="([^"]*)"[^>]*>[^<]*<\/div>/gi, (_, pdfSrc, name) => {
			const vaultRoot = $appConfig?.active_vault ?? '';
			const absPath = normalizePath(`${vaultRoot}/${decodeURIComponent(pdfSrc)}`);
			const showInline = !isMobile && ($appConfig?.pdf_preview ?? false);
			if (showInline) {
				const pdfHeight = $appConfig?.pdf_height ?? 600;
				const displaySrc = convertFileSrc(absPath);
				return `<div data-pdf-src="${pdfSrc}" data-pdf-name="${name}" class="pdf-embed"><iframe src="${displaySrc}" width="100%" height="${pdfHeight}px"></iframe><p class="pdf-label">${name}</p></div>`;
			}
			return `<div data-pdf-src="${pdfSrc}" data-pdf-name="${name}" class="pdf-embed-mobile"><a href="${decodeURIComponent(pdfSrc)}" class="pdf-link-mobile">\uD83D\uDCC4 ${name}</a></div>`;
		});

		// Pre-process: render KaTeX math - only outside fenced code blocks
		{
			const lines = src.split('\n');
			const outLines: string[] = [];
			let inFence = false;
			let mathBlock: string[] | null = null;
			for (let i = 0; i < lines.length; i++) {
				const line = lines[i];
				if (/^```/.test(line)) { inFence = !inFence; outLines.push(line); continue; }
				if (inFence) { outLines.push(line); continue; }
				// Accumulate block math: $$ on its own line starts/ends a block
				if (line.trim() === '$$') {
					if (!mathBlock) { mathBlock = []; continue; }
					const tex = mathBlock.join('\n').trim();
					mathBlock = null;
					outLines.push(`<div data-math-block="${encodeURIComponent(tex)}" class="math-block"></div>`);
					continue;
				}
				if (mathBlock) { mathBlock.push(line); continue; }
				// Inline math: $...$ (skip content inside backticks)
				const processed = line.replace(/`[^`]*`/g, m => '\x00'.repeat(m.length));
				let result = line;
				let offset = 0;
				for (const m of processed.matchAll(/(?<!\$)\$(?![\s$])([^\n$]+?)(?<!\s)\$(?!\$)(?!\d)/g)) {
					const tex = m[1].trim();
					const html = `<span data-math-inline="${encodeURIComponent(tex)}" class="math-inline"></span>`;
					result = result.slice(0, m.index! + offset) + html + result.slice(m.index! + m[0].length + offset);
					offset += html.length - m[0].length;
				}
				outLines.push(result);
			}
			// If unclosed math block, just output the lines as-is
			if (mathBlock) { outLines.push('$$', ...mathBlock); }
			src = outLines.join('\n');
		}

		// Pre-process: convert task list syntax before markdown-it (it doesn't know TipTap's format)
		// Support indented (nested) and blockquoted task lists too
		src = src.replace(/^([\s>]*)-\s\[x\][^\S\n]+(.+)$/gm, '$1- <tiptask checked="true">$2</tiptask>');
		src = src.replace(/^([\s>]*)-\s\[x\][^\S\n]*$/gm, '$1- <tiptask checked="true">&nbsp;</tiptask>');
		src = src.replace(/^([\s>]*)-\s\[ \][^\S\n]+(.+)$/gm, '$1- <tiptask checked="false">$2</tiptask>');
		src = src.replace(/^([\s>]*)-\s\[ \][^\S\n]*$/gm, '$1- <tiptask checked="false">&nbsp;</tiptask>');

		// Pre-process: strip list-separator comments before markdown-it.
		// markdown-it treats <!-- --> as an HTML block start, swallowing the
		// next line (e.g. an image) as raw HTML instead of parsing it.
		src = src.replace(/<!-- -->/g, '\n');

		// Pre-process: preserve blank lines before image-only lines
		// markdown-it collapses blank lines into paragraph breaks, losing the empty paragraph.
		// Insert a <div> marker that markdown-it passes through (html: true), then convert to <p></p>
		src = src.replace(/\n\n(!\[[^\]]*\]\([^)]*\)\s*$)/gm, '\n\n<div data-img-gap></div>\n\n$1');

		// Run markdown-it (single-pass parser - handles headings, bold, italic, strike, code, blockquote, lists, links, images, hr, tables, raw HTML)
		let html = mdit.render(src);

		// Post-process: convert image gap markers into empty paragraphs for ProseMirror
		html = html.replace(/<div data-img-gap><\/div>\n?/g, '<p></p>\n');

		// Post-process: strip trailing newlines inside code blocks (markdown-it adds them, TipTap shows them as blank lines)
		html = html.replace(/<code([^>]*)>\n?/g, '<code$1>');
		html = html.replace(/\n<\/code>/g, '</code>');

		// Post-process: convert list-separator comments back to empty paragraphs for TipTap
		html = html.replace(/<!-- -->/g, '<p></p>');

		// Post-process: convert task list items to TipTap format
		// Convert opening <li> + <tiptask> into data-attributed <li>, handles both tight and loose (with <p>) lists
		html = html.replace(/<li>(\s*(?:<p>)?)\s*<tiptask checked="(true|false)">([\s\S]*?)<\/tiptask>\s*(?:<\/p>)?/gi, (_, _pre, checked, text) => {
			return `<li data-type="taskItem" data-checked="${checked}">${text}`;
		});
		html = html.replace(/<ul>(\s*<li data-type="taskItem")/gi, '<ul data-type="taskList">$1');

		// Post-process: resolve image src paths and parse size attribute
		html = html.replace(/<img\s+src="([^"]*)"(?:\s+alt="([^"]*)")?[^>]*\/?>/gi, (_, imgSrc, altRaw) => {
			let alt = altRaw || '';
			let size = 'full';
			const sizeMatch = alt.match(/^(.*?)\|size=(small|medium|full)$/);
			if (sizeMatch) {
				alt = sizeMatch[1];
				size = sizeMatch[2];
			}
			return `<img src="${resolveImageSrc(imgSrc)}" alt="${alt}" data-size="${size}">`;
		});

		// Post-process: turn Obsidian callout blockquotes (> [!type] ...) into callout blocks.
		if (html.includes('[!')) {
			const tmp = document.createElement('div');
			tmp.innerHTML = html;
			transformCalloutBlockquotes(tmp);
			html = tmp.innerHTML;
		}

		return html;
	}

	function escapeHtml(str: string): string {
		return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
	}

	// When editorElement appears in DOM, initialize TipTap.
	// On mobile, pre-create editor with empty content so first note load is fast.
	$effect(() => {
		if (editorElement && !editor) {
			if (pendingContent !== null) {
				createEditor(pendingContent);
				pendingContent = null;
				if (editor) {
					const text = (editor as any).state.doc.textContent;
					wordCount = countWords(text);
					charCount = text.replace(/\s/g, '').length;
				}
			} else if (isMobile && !lastSourceMode) {
				createEditor('');
			}
		}
	});

	// Close formatting dropdowns when clicking outside the formatting bar
	$effect(() => {
		if (!anyDropdownOpen) return;
		function onClickAway(e: MouseEvent) {
			const bar = document.querySelector('.editor-formatting-bar');
			if (bar && !bar.contains(e.target as Node)) {
				closeAllDropdowns();
			}
		}
		document.addEventListener('mousedown', onClickAway);
		return () => document.removeEventListener('mousedown', onClickAway);
	});

	// Live word/char count in source mode
	$effect(() => {
		if ($sourceMode) updateCounts();
	});

	// Auto-close info panel on click outside
	$effect(() => {
		if (!showInfo) return;
		function onInfoClickAway(e: MouseEvent) {
			if (
				infoPanelEl && !infoPanelEl.contains(e.target as Node) &&
				infoToggleBtnEl && !infoToggleBtnEl.contains(e.target as Node)
			) {
				showInfo = false;
			}
		}
		document.addEventListener('mousedown', onInfoClickAway);
		return () => document.removeEventListener('mousedown', onInfoClickAway);
	});

	// Close in-note search when switching notes
	let prevSearchPath = '';
	$effect(() => {
		const path = $activeNotePath ?? '';
		if (prevSearchPath && path !== prevSearchPath) {
			noteSearchOpen = false;
			noteSearchQuery = '';
			noteSearchResults = [];
			noteSearchIndex = 0;
		}
		prevSearchPath = path;
	});

	// React to activeNotePath changes from external sources (e.g. search panel)
	$effect(() => {
		const path = $activeNotePath;
		const note = $activeNote;
		if (!path) {
			// Note was deselected (e.g. deleted) - destroy editor so it reinits on next note
			destroyEditor();
			loadedPath = '';
			return;
		}
		if (note && path !== loadedPath) {
			loadNote(path, note.content);
		}
	});

	function destroyEditor() {
		flushSave();
		if (editor) {
			editor.destroy();
			editor = null;
		}
		mathObserver?.disconnect();
		mathObserver = null;
		editorReady = false;
		closeSlashMenu();
	}

	function createEditor(content: string) {
		if (!editorElement) return;
		if (editor) {
			editor.destroy();
			editor = null;
		}
		mathObserver?.disconnect();
		mathObserver = null;

		isLargeDoc = content.length > LARGE_DOC_CHARS;
		const html = markdownToHtml(content);

		editor = new Editor({
			element: editorElement,
			editable: !$readOnly,
			extensions: [
				StarterKit.configure({ codeBlock: false }),
				Placeholder.configure({
					includeChildren: true,
					placeholder: ({ node }) => {
						if (node.type.name === 'detailsSummary') return 'Section title...';
						if (node.type.name === 'detailsContent') return 'Content...';
						return 'Start writing...';
					},
				}),
				TaskList,
				TaskItem.configure({ nested: true }),
				Table.configure({ resizable: true }),
				TableRow,
				CustomTableCell,
				CustomTableHeader,
				Link.configure({ openOnClick: false, HTMLAttributes: { class: 'editor-link' }, isAllowedUri: (url, ctx) => ctx.defaultValidate(url) || !url.startsWith('javascript:'), shouldAutoLink: (url) => /^https?:\/\//.test(url) }),
				CustomImage.configure({ inline: true, HTMLAttributes: { class: 'editor-image' } }),
				Highlight.configure({ multicolor: true }),
				Typography,
				Underline,
				Subscript,
				Superscript,
				TextStyle,
				Color,
				CodeBlockLowlight.configure({ lowlight, enableTabIndentation: true, defaultLanguage: 'text' }),
				CodeBlockLanguageSelect,
				CopyButtonExtension,
				MermaidRenderer,
				PdfEmbed,
				SecretBlock,
				MathBlock,
				MathInline,
				PageBreak,
				Callout,
				Details.configure({ persist: true, HTMLAttributes: { class: 'editor-details' } }),
				DetailsSummary,
				DetailsContent,
				Extension.create({
					name: 'collapsibleKeymap',
					addProseMirrorPlugins() {
						return [new Plugin({
							key: new PluginKey('collapsibleKeymap'),
							props: {
								handleDOMEvents: {
									keydown(view, event) {
										const isTab = event.key === 'Tab' && !event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
										const isEnter = event.key === 'Enter' && !event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey;
										if (!isTab && !isEnter) return false;
										const { schema, selection } = view.state;
										const from = selection.$from;
										let summaryDepth = -1;
										for (let d = from.depth; d >= 0; d--) {
											if (from.node(d).type === schema.nodes.detailsSummary) { summaryDepth = d; break; }
										}
										if (summaryDepth === -1) return false;
										event.preventDefault();
										const detailsDepth = summaryDepth - 1;
										const detailsNode = from.node(detailsDepth);
										let detailsContentPos: number | null = null;
										let pos = from.start(detailsDepth);
										for (let i = 0; i < detailsNode.childCount; i++) {
											const child = detailsNode.child(i);
											if (child.type === schema.nodes.detailsContent) { detailsContentPos = pos + 1; break; }
											pos += child.nodeSize;
										}
										if (detailsContentPos === null) return true;
										// Open the section if it is closed
										const domPos = view.domAtPos(from.pos);
										let domNode = domPos.node as HTMLElement;
										if (domNode.nodeType === 3) domNode = domNode.parentElement as HTMLElement;
										const detailsEl = domNode?.closest('[data-type="details"]') as HTMLElement | null;
										if (detailsEl) openDetailsEl(detailsEl);
										// Sync open state into document + move cursor (single transaction)
										const detailsPos = from.before(detailsDepth);
										const tr = view.state.tr.setNodeMarkup(detailsPos, undefined, { open: true });
										const mappedContentPos = tr.mapping.map(detailsContentPos);
										tr.setSelection(Selection.near(tr.doc.resolve(mappedContentPos), 1));
										view.dispatch(tr.scrollIntoView());
										view.focus();
										return true;
									}
								}
							}
						})];
					}
				}),
				Extension.create({
					name: 'detailsOpenAttrSync',
					addProseMirrorPlugins() {
						return [new Plugin({
							key: new PluginKey('detailsOpenAttrSync'),
							props: {
								handleDOMEvents: {
									click(view, event) {
										const btn = (event.target as HTMLElement)?.closest?.('button');
										const detailsEl = btn && btn.parentElement?.getAttribute('data-type') === 'details'
											? (btn.parentElement as HTMLElement) : null;
										if (!detailsEl) return false;
										// The extension's button handler already toggled the is-open class; sync
										// node.attrs.open to it. Fixes the first node (pos 0), where upstream's
										// `if (!pos)` guard skips persisting the attribute.
										const isOpen = detailsEl.classList.contains('is-open');
										const probe = detailsEl.querySelector('[data-type="detailsContent"]') ?? detailsEl;
										let pos: number;
										try { pos = view.posAtDOM(probe, 0); } catch { return false; }
										const resolved = view.state.doc.resolve(pos);
										for (let d = resolved.depth; d >= 0; d--) {
											if (resolved.node(d).type.name === 'details') {
												if (resolved.node(d).attrs.open !== isOpen) {
													view.dispatch(view.state.tr.setNodeMarkup(resolved.before(d), undefined, { open: isOpen }));
												}
												break;
											}
										}
										return false;
									}
								}
							}
						})];
					}
				}),
				TextAlign.configure({ types: ['heading', 'paragraph'] }).extend({
					addKeyboardShortcuts: () => ({}),
				}),
				CtrlEndScrollPastEnd,
				HeadingShortcuts,
				WrapSelectedText,
				SlashCommands,
				TaskMetaMenu,
				MoveLineShortcuts,
				TabIndent,
				NoteSearchExtension,
				ColorSwatch,
				TaskMetaDim,
				...($appConfig?.enable_wiki_links ? [WikiLink, WikiLinkAutocomplete] : []),
			],
			content: html,
			editorProps: {
				attributes: { class: 'editor-content', spellcheck: 'false' },
				handleDOMEvents: {
					// Prevent focus-caused scroll jumps when clicking details toggle buttons.
					// Pre-focusing with preventScroll means TipTap's focus() call sees
					// hasFocus()=true and skips its scrolling view.focus() call.
					// For task checkboxes: lock scroll on mousedown (before any dispatch fires)
					// so that any synchronous or async scroll caused by the toggle is reverted.
					mousedown: (view, event) => {
						const target = event.target as HTMLElement;
						if (target.closest('[data-type="details"] > button')) {
							event.preventDefault();
							if (!view.hasFocus()) {
								(view.dom as HTMLElement).focus({ preventScroll: true });
							}
						}
						if (target.closest('li[data-checked] > label')) {
							const editorBody = target.closest('.editor-body') as HTMLElement | null;
							if (editorBody) {
								const savedScroll = editorBody.scrollTop;
								const restore = () => { editorBody!.scrollTop = savedScroll; };
								editorBody.addEventListener('scroll', restore);
								// Remove after 200ms - covers synchronous, rAF, and setTimeout-based scrolls
								setTimeout(() => editorBody!.removeEventListener('scroll', restore), 200);
							}
						}
					},
					// Let task-list checkboxes be ticked in View Mode (read-only) without
					// entering edit mode - e.g. a shopping list on mobile, where editing would
					// pop the soft keyboard. TipTap leaves the checkbox clickable in read-only
					// but reverts the toggle on `change`; we intercept the click, cancel the
					// native toggle, and dispatch the attr change ourselves so it persists via
					// the normal onUpdate -> autoSave path. Edit mode keeps TipTap's behaviour.
					click: (view, event) => {
						if (!get(readOnly)) return false;
						if (get(viewerNote)) return false; // external viewer files are never saved
						const target = event.target as HTMLElement;
						const label = target.closest('li[data-checked] > label');
						if (!label) return false;
						// Cancel the native checkbox toggle (and the label->input click cascade)
						// so it doesn't fight our transaction; the node-view update will set the
						// visual state from the new doc value.
						event.preventDefault();
						const li = label.closest('li') as HTMLElement | null;
						if (!li) return false;
						// Probe position from the content <div> (the real contentDOM), not the <li>
						// outer node-view element, so posAtDOM lands inside the task item's content.
						const probe = (li.querySelector(':scope > div') as HTMLElement | null) || li;
						const pos = view.posAtDOM(probe, 0);
						const resolved = view.state.doc.resolve(pos);
						let itemPos = -1;
						let attrs: Record<string, any> | null = null;
						for (let d = resolved.depth; d >= 0; d--) {
							if (resolved.node(d).type.name === 'taskItem') {
								itemPos = resolved.before(d);
								attrs = resolved.node(d).attrs;
								break;
							}
						}
						if (itemPos < 0) {
							// Fallback: pos landed just before the item rather than inside it.
							const n = view.state.doc.nodeAt(pos);
							if (n && n.type.name === 'taskItem') { itemPos = pos; attrs = n.attrs; }
						}
						if (itemPos < 0 || !attrs) return false;
						view.dispatch(view.state.tr.setNodeMarkup(itemPos, undefined, { ...attrs, checked: !attrs.checked }));
						return true;
					},
					// Prevent native text drag - it causes copy-instead-of-move in Tauri's webview.
					// File drops from OS are handled by Tauri's onDragDropEvent listener instead.
					dragstart: (_view, event) => {
						const dt = event.dataTransfer;
						if (!dt || dt.files.length === 0) {
							event.preventDefault();
						}
					},
				},
				handleDrop: (_view, event) => handleFileDrop(event),
				handlePaste: (_view, event) => {
					const handled = handleFilePaste(event);
					if (!handled) hasPendingBlobs = true; // ProseMirror may insert blob: images from web paste
					return handled;
				},
				// Strip color / font styling from pasted HTML so the editor uses its own theme.
				// Keeps semantic marks (bold, italic, links, headings, alignment) - drops only the
				// inline visual styles that fight the app's theme (color, bg-color, font-family, font-size).
				transformPastedHTML: (html: string) => {
					if (!/style=|<font/i.test(html)) return html;
					try {
						const doc = new DOMParser().parseFromString(html, 'text/html');
						doc.querySelectorAll('[style]').forEach((el) => {
							const style = (el as HTMLElement).style;
							style.color = '';
							style.backgroundColor = '';
							style.fontFamily = '';
							style.fontSize = '';
							if (!style.cssText.trim()) el.removeAttribute('style');
						});
						doc.querySelectorAll('font').forEach((el) => {
							el.removeAttribute('color');
							el.removeAttribute('face');
							el.removeAttribute('size');
						});
						return doc.body.innerHTML;
					} catch (e) {
						console.warn('[paste] style strip failed', e);
						return html;
					}
				},
			},
			onTransaction: () => {
				// Batch toolbar state updates to once per frame - avoids ~35 isActive() calls per transaction during selection drag
				if (!editorStateRaf) {
					editorStateRaf = requestAnimationFrame(() => {
						editorStateRaf = 0;
						editorState++;
					});
				}
				// On mobile, only check menus when they're already open or user just typed trigger char
				if (!isMobile || slashMenu || slashTypedByUser) updateSlashMenu();
				if (!isMobile || wikiLinkMenu || wikiLinkTypedByUser) updateWikiLinkMenu();
				if (!isMobile || taskMetaMenu || taskMetaTypedByUser) updateTaskMetaMenu();
				// Detect when cursor leaves a wiki-link mark and trigger rename check
				if ($appConfig?.enable_wiki_links && editor) {
					const curMarks = editor.state.selection.$from.marks();
					const curWikiMark = curMarks.find((m: any) => m.type.name === 'wikiLink') ?? null;
					if (prevCursorWikiMark && !curWikiMark) {
						checkWikiLinkRenames();
					}
					prevCursorWikiMark = curWikiMark;
				}
			},
			onUpdate: () => {
				if (ignoreNextUpdate || isLoadingNote) {
					ignoreNextUpdate = false;
					return;
				}
				$editorDirty = true;
				autoSave();
				if (!isMobile && showOutline) scheduleOutline();
				if (showInfo) scheduleCounts();
			},
		});
		editorReady = true;
		// Pre-load note titles for wiki-link autocomplete
		if ($appConfig?.enable_wiki_links) {
			refreshWikiLinkTitles();
		}
	}

	export function toggleOutlinePanel() {
		showOutline = !showOutline;
		if (showOutline) updateOutline();
	}

	export function toggleHistoryPanel() {
		toggleHistory();
	}

	export function triggerAiMenu() {
		openAiMenu();
	}

	export function toggleGraphView() {
		showGraph = !showGraph;
	}

	export function addLinkFromToolbar() {
		if (!editor) return;
		const { from, to } = editor.state.selection;
		linkSelectionFrom = from;
		linkSelectionTo = to;
		const previousUrl = editor.getAttributes('link').href || '';
		linkModalUrl = decodeURIComponent(previousUrl);
		linkSuggestIndex = 0;
		linkModal = true;
		// Load note titles for autocomplete
		getAllNoteTitles().then(t => { linkSuggestTitles = t; }).catch(() => {});
		tick().then(() => linkModalInput?.focus());
	}

	function linkModalConfirm() {
		if (!editor) return;
		let url = linkModalUrl.trim();
		if (url === '') {
			editor.chain().focus().setTextSelection({ from: linkSelectionFrom, to: linkSelectionTo }).extendMarkRange('link').unsetLink().run();
		} else {
			if (url && !/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(url) && !url.startsWith('/') && !url.startsWith('#') && !url.endsWith('.md')) {
				url = 'https://' + url;
			}
			// Store raw URL - encoding is handled during markdown serialization/parsing
			const href = url.replace(/[()]/g, (c) => encodeURIComponent(c));
			if (linkSelectionFrom === linkSelectionTo) {
				const text = linkModalDisplayText || url.replace(/\.md$/, '').split('/').pop() || url;
				editor.chain().focus().setTextSelection(linkSelectionFrom).insertContent({
					type: 'text',
					text,
					marks: [{ type: 'link', attrs: { href } }],
				}).run();
			} else {
				editor.chain().focus().setTextSelection({ from: linkSelectionFrom, to: linkSelectionTo }).setMark('link', { href }).run();
			}
		}
		linkModal = false;
		linkModalUrl = '';
		linkModalDisplayText = '';
	}

	function linkModalSelectNote(entry: NoteTitleEntry) {
		// Build a relative .md path from the selected note and confirm immediately
		const vaultRoot = $appConfig?.active_vault;
		const currentNote = $activeNotePath;
		if (vaultRoot && currentNote) {
			const noteDir = currentNote.substring(0, currentNote.lastIndexOf('/'));
			const targetRel = entry.path.startsWith(vaultRoot) ? entry.path.substring(vaultRoot.length + 1) : entry.path;
			const currentRel = noteDir.startsWith(vaultRoot) ? noteDir.substring(vaultRoot.length + 1) : noteDir;
			const targetParts = targetRel.split('/');
			const currentParts = currentRel ? currentRel.split('/') : [];
			let common = 0;
			while (common < targetParts.length && common < currentParts.length && targetParts[common] === currentParts[common]) common++;
			const ups = currentParts.length - common;
			linkModalUrl = (ups > 0 ? '../'.repeat(ups) : './') + targetParts.slice(common).join('/');
		} else {
			linkModalUrl = entry.title + '.md';
		}
		linkModalDisplayText = entry.title;
		linkModalConfirm();
	}

	function linkModalCancel() {
		linkModal = false;
		linkModalUrl = '';
		linkModalDisplayText = '';
		editor?.chain().focus().run();
	}

	function insertTable(rows: number, cols: number) {
		if (!editor) return;
		editor.chain().focus().insertTable({ rows, cols, withHeaderRow: true }).run();
		tablePickerOpen = false;
		tablePickerHover = { rows: 0, cols: 0 };
	}

	function setTextColor(color: string) {
		if (!editor) return;
		if (color === '') {
			editor.chain().focus().unsetColor().run();
		} else {
			editor.chain().focus().setColor(color).run();
		}
		colorDropdown = false;
	}

	function setHighlightColor(color: string) {
		if (!editor) return;
		if (color === '') {
			editor.chain().focus().unsetHighlight().run();
		} else {
			editor.chain().focus().setHighlight({ color }).run();
		}
		highlightDropdown = false;
	}

	function handleEditorClick(event: MouseEvent) {
		const target = event.target as HTMLElement;

		const wikiLinkEl = target.closest('span[data-wiki-link]') as HTMLElement | null;
		if (wikiLinkEl) {
			imageToolbar = null;
			event.preventDefault();
			event.stopPropagation();
			const path = wikiLinkEl.getAttribute('data-path') || '';
			const title = wikiLinkEl.textContent || wikiLinkEl.getAttribute('data-title') || '';
			navigateToWikiLink(path, title, event);
			return;
		}

		if (target.tagName === 'IMG' && editor) {
			event.preventDefault();
			event.stopPropagation();
			const pos = editor.view.posAtDOM(target, 0);
			// If toolbar is already open for this image, close it
			if (imageToolbar && imageToolbar.pos === pos) {
				imageToolbar = null;
				return;
			}
			const node = editor.state.doc.nodeAt(pos);
			const currentSize = node?.attrs.size || 'full';
			const imgSrc = node?.attrs.src || (target as HTMLImageElement).src || '';
			const toolbarW = isMobile ? 130 : 250;
			const toolbarH = 38;
			const x = Math.min(event.clientX, window.innerWidth - toolbarW - 8);
			const y = Math.min(event.clientY, window.innerHeight - toolbarH - 8);
			imageToolbar = { pos, x, y, size: currentSize, src: imgSrc };
			// Move cursor after the image to clear ProseMirror's node selection highlight
			const afterPos = pos + (node?.nodeSize || 1);
			editor.chain().setTextSelection(afterPos).run();
			return;
		}

		imageToolbar = null;
	}

	function setImageSize(size: string) {
		if (!editor || !imageToolbar) return;
		const { pos } = imageToolbar;
		const tr = editor.state.tr.setNodeAttribute(pos, 'size', size);
		editor.view.dispatch(tr);
		imageToolbar = { ...imageToolbar, size };
		$editorDirty = true;
		autoSave();
	}

	function getImageAbsPath(src: string): string {
		// asset:// or http://asset.localhost → extract absolute path
		if (src.startsWith('asset:') || src.startsWith('http://asset.localhost')) {
			try {
				const url = new URL(src);
				let absPath = decodeURIComponent(url.pathname);
				absPath = absPath.replace(/^\/{2,}/, '/');
				return absPath;
			} catch { /* fall through */ }
		}
		// Relative path → resolve against note directory
		let decoded = decodeURIComponent(src);
		if (decoded.match(/^\/{2,}/)) decoded = decoded.replace(/^\/{2,}/, '/');
		if (decoded.startsWith('/')) return decoded;
		if (decoded.includes('.helixnotes/')) {
			const vaultRoot = $appConfig?.active_vault;
			if (vaultRoot) {
				const idx = decoded.indexOf('.helixnotes/');
				return `${vaultRoot}/${decoded.substring(idx)}`;
			}
		}
		const notePath = $activeNotePath;
		if (notePath) {
			const noteDir = notePath.substring(0, notePath.lastIndexOf('/'));
			return normalizePath(`${noteDir}/${decoded}`);
		}
		const vaultRoot = $appConfig?.active_vault;
		if (vaultRoot) return normalizePath(`${vaultRoot}/${decoded}`);
		return src;
	}

	async function copyImageToClipboard() {
		if (!imageToolbar) return;
		const absPath = getImageAbsPath(imageToolbar.src);
		imageToolbar = null;
		copyToast = 'copying';
		// Yield to let Svelte render the "Copying..." toast before blocking on IPC
		await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
		try {
			await copyImageToClipboardCmd(absPath);
			copyToast = 'done';
		} catch (e) {
			console.error('Failed to copy image:', e);
			copyToast = null;
			return;
		}
		setTimeout(() => { copyToast = null; }, 1000);
	}

	function openImageInApp() {
		if (!imageToolbar) return;
		const absPath = getImageAbsPath(imageToolbar.src);
		openFile(absPath).catch(e => console.error('Failed to open image:', e));
		imageToolbar = null;
	}

	function handleEditorContextMenu(event: MouseEvent) {
		const target = event.target as HTMLElement;
		const anchor = target.closest('a');
		if (anchor) {
			const href = anchor.getAttribute('href');
			if (href) {
				event.preventDefault();
				event.stopPropagation();
				let lx = event.clientX;
				let ly = event.clientY;
				const lw = 200, lh = 200;
				if (lx + lw > window.innerWidth) lx = window.innerWidth - lw - 8;
				if (ly + lh > window.innerHeight) ly = window.innerHeight - lh - 8;
				if (lx < 4) lx = 4;
				if (ly < 4) ly = 4;
				linkContextMenu = { x: lx, y: ly, href, anchor };
				return;
			}
		}
		// Check if inside a table cell
		const cell = target.closest('td, th');
		if (cell) {
			event.preventDefault();
			event.stopPropagation();
			let x = event.clientX;
			let y = event.clientY;
			const menuWidth = 220;
			const menuHeight = 600;
			if (x + menuWidth > window.innerWidth) x = window.innerWidth - menuWidth - 8;
			if (y + menuHeight > window.innerHeight) y = window.innerHeight - menuHeight - 8;
			if (x < 4) x = 4;
			if (y < 4) y = 4;
			let hasStyling = false;
			if (editor) {
				const pos = editor.view.posAtDOM(event.target as Node, 0);
				let resolved = editor.state.doc.resolve(pos);
				for (let d = resolved.depth; d >= 0; d--) {
					if (resolved.node(d).type.name === 'table') {
						resolved.node(d).descendants((n: any) => {
							if (n.type.name === 'tableCell' || n.type.name === 'tableHeader') {
								if (n.attrs.backgroundColor || (n.attrs.colspan > 1) || (n.attrs.rowspan > 1)) hasStyling = true;
							}
							return true;
						});
						break;
					}
				}
			}
			tableContextMenu = { x, y, hasStyling };
			return;
		}
		// Show text context menu for any right-click in editor
		event.preventDefault();
		event.stopPropagation();
		// Position menu, adjusting if it would overflow the viewport
		let x = event.clientX;
		let y = event.clientY;
		const menuWidth = 220;
		const menuHeight = 740;
		if (x + menuWidth > window.innerWidth) x = window.innerWidth - menuWidth - 8;
		if (y + menuHeight > window.innerHeight) y = window.innerHeight - menuHeight - 8;
		if (x < 4) x = 4;
		if (y < 4) y = 4;
		const submenuWidth = 150;
		const submenuLeft = x + menuWidth + submenuWidth > window.innerWidth;
		textContextMenu = { x, y, submenuLeft };
	}

	function closeTextContextMenu() {
		textContextMenu = null;
		ctxHeadingSubmenu = false;
	}

	function closeTableContextMenu() {
		tableContextMenu = null;
	}

	function tblAddRowBefore() { editor?.chain().focus().addRowBefore().run(); closeTableContextMenu(); }
	function tblAddRowAfter() { editor?.chain().focus().addRowAfter().run(); closeTableContextMenu(); }
	function tblDeleteRow() { editor?.chain().focus().deleteRow().run(); closeTableContextMenu(); }
	function tblAddColBefore() { editor?.chain().focus().addColumnBefore().run(); closeTableContextMenu(); }
	function tblAddColAfter() { editor?.chain().focus().addColumnAfter().run(); closeTableContextMenu(); }
	function tblDeleteCol() { editor?.chain().focus().deleteColumn().run(); closeTableContextMenu(); }
	function tblMergeCells() { editor?.chain().focus().mergeCells().run(); closeTableContextMenu(); }
	function tblSplitCell() { editor?.chain().focus().splitCell().run(); closeTableContextMenu(); }
	function tblToggleHeaderRow() { editor?.chain().focus().toggleHeaderRow().run(); closeTableContextMenu(); }
	function tblToggleHeaderCol() { editor?.chain().focus().toggleHeaderColumn().run(); closeTableContextMenu(); }
	function tblDeleteTable() { editor?.chain().focus().deleteTable().run(); closeTableContextMenu(); }
	function tblSetCellColor(color: string) {
		if (!editor) return;
		if (color === '') {
			editor.chain().focus().setCellAttribute('backgroundColor', null).run();
		} else {
			editor.chain().focus().setCellAttribute('backgroundColor', color).run();
		}
		closeTableContextMenu();
	}

	async function ctxCut() {
		if (!editor) return;
		const { from, to } = editor.state.selection;
		if (from === to) { closeTextContextMenu(); return; }
		const text = editor.state.doc.textBetween(from, to, '\n');
		await navigator.clipboard.writeText(text);
		editor.chain().focus().deleteSelection().run();
		closeTextContextMenu();
	}

	async function ctxCopy() {
		if (!editor) return;
		const { from, to } = editor.state.selection;
		if (from === to) { closeTextContextMenu(); return; }
		const text = editor.state.doc.textBetween(from, to, '\n');
		await navigator.clipboard.writeText(text);
		closeTextContextMenu();
	}

	async function ctxPaste() {
		if (!editor) return;
		try {
			const text = await navigator.clipboard.readText();
			if (text) editor.chain().focus().insertContent(text).run();
		} catch (e) {
			console.error('Paste failed:', e);
		}
		closeTextContextMenu();
	}

	function ctxSelectAll() {
		if (!editor) return;
		editor.chain().focus().selectAll().run();
		closeTextContextMenu();
	}

	let ctxHeadingSubmenu = $state(false);

	function ctxSetHeading(level: number) {
		editor?.chain().focus().toggleHeading({ level: level as 1 | 2 | 3 | 4 }).run();
		closeTextContextMenu();
	}

	function ctxSetParagraph() {
		editor?.chain().focus().setParagraph().run();
		closeTextContextMenu();
	}

	function ctxBold() {
		editor?.chain().focus().toggleBold().run();
		closeTextContextMenu();
	}

	function ctxItalic() {
		editor?.chain().focus().toggleItalic().run();
		closeTextContextMenu();
	}

	function ctxUnderline() {
		editor?.chain().focus().toggleUnderline().run();
		closeTextContextMenu();
	}

	function ctxStrike() {
		editor?.chain().focus().toggleStrike().run();
		closeTextContextMenu();
	}

	function ctxLink() {
		closeTextContextMenu();
		addLinkFromToolbar();
	}

	function ctxHighlight() {
		editor?.chain().focus().toggleHighlight({ color: highlightColors[0].value }).run();
		closeTextContextMenu();
	}

	function ctxCode() {
		editor?.chain().focus().toggleCode().run();
		closeTextContextMenu();
	}

	function ctxCodeBlock() {
		editor?.chain().focus().toggleCodeBlock().run();
		closeTextContextMenu();
	}

	function ctxBlockquote() {
		editor?.chain().focus().toggleBlockquote().run();
		closeTextContextMenu();
	}

	function ctxTimestamp() {
		insertTimestamp('datetime');
		closeTextContextMenu();
	}

	function openDetailsEl(el: HTMLElement) {
		if (!el.classList.contains('is-open')) {
			el.classList.add('is-open');
			(el.querySelector('[data-type="detailsContent"]') as HTMLElement | null)
				?.dispatchEvent(new Event('toggleDetailsContent'));
		}
	}

	function insertCallout(type = 'note') {
		if (!editor) return;
		editor.chain().focus().wrapIn('callout', { type, title: '', foldable: false, folded: false }).run();
	}

	function openCalloutTypeMenu(anchor: HTMLElement, onPick: (type: string) => void) {
		document.querySelectorAll('.callout-type-menu').forEach((el) => el.remove());
		const menu = document.createElement('div');
		menu.className = 'callout-type-menu';
		const close = () => {
			menu.remove();
			document.removeEventListener('mousedown', onDoc, true);
			document.removeEventListener('keydown', onKey, true);
			window.removeEventListener('scroll', close, true);
		};
		const onDoc = (e: MouseEvent) => { if (!menu.contains(e.target as Node)) close(); };
		const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close(); };
		for (const item of CALLOUT_MENU) {
			const btn = document.createElement('button');
			btn.type = 'button';
			btn.className = 'callout-type-option';
			btn.innerHTML = `<span class="callout-type-icon" style="color: rgb(var(--callout-${item.type}))">${calloutIcon(item.type, 16)}</span><span>${item.label}</span>`;
			btn.addEventListener('mousedown', (e) => e.preventDefault());
			btn.addEventListener('click', () => { onPick(item.type); close(); });
			menu.appendChild(btn);
		}
		// "Custom…" fills the last grid slot: name any callout type (Obsidian round-trips it).
		const customBtn = document.createElement('button');
		customBtn.type = 'button';
		customBtn.className = 'callout-type-option';
		customBtn.innerHTML = '<span class="callout-type-icon" style="color: var(--text-secondary)"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="21" x2="4" y2="14"/><line x1="4" y1="10" x2="4" y2="3"/><line x1="12" y1="21" x2="12" y2="12"/><line x1="12" y1="8" x2="12" y2="3"/><line x1="20" y1="21" x2="20" y2="16"/><line x1="20" y1="12" x2="20" y2="3"/><line x1="1" y1="14" x2="7" y2="14"/><line x1="9" y1="8" x2="15" y2="8"/><line x1="17" y1="16" x2="23" y2="16"/></svg></span><span>Custom…</span>';
		customBtn.addEventListener('mousedown', (e) => e.preventDefault());
		customBtn.addEventListener('click', () => {
			menu.classList.add('is-custom');
			menu.innerHTML = '';
			const input = document.createElement('input');
			input.className = 'callout-type-custom-input';
			input.placeholder = 'Type name, then Enter';
			input.spellcheck = false;
			input.addEventListener('keydown', (e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					const v = input.value.trim().toLowerCase().replace(/[^\w-]/g, '');
					if (v) onPick(v);
					close();
				} else if (e.key === 'Escape') {
					e.preventDefault();
					close();
				}
			});
			menu.appendChild(input);
			input.focus();
		});
		menu.appendChild(customBtn);
		document.body.appendChild(menu);
		const r = anchor.getBoundingClientRect();
		menu.style.top = `${Math.min(r.bottom + 4, window.innerHeight - menu.offsetHeight - 8)}px`;
		menu.style.left = `${Math.min(r.left, window.innerWidth - menu.offsetWidth - 8)}px`;
		setTimeout(() => {
			document.addEventListener('mousedown', onDoc, true);
			document.addEventListener('keydown', onKey, true);
			window.addEventListener('scroll', close, true);
		}, 0);
	}

	function insertDetails() {
		if (!editor) return;
		editor.chain().focus().setDetails().run();
		requestAnimationFrame(() => {
			if (!editor) return;
			const domPos = editor.view.domAtPos(editor.state.selection.from);
			let node = domPos.node as HTMLElement;
			if (node.nodeType === 3) node = node.parentElement as HTMLElement;
			const detailsEl = node.closest('[data-type="details"]') as HTMLElement | null;
			if (detailsEl) openDetailsEl(detailsEl);
			// Sync open: true into the document so it saves with the note
			editor.chain().updateAttributes('details', { open: true }).run();
		});
	}

	function ctxDetails() {
		insertDetails();
		closeTextContextMenu();
	}

	function ctxCallout() {
		insertCallout('note');
		closeTextContextMenu();
	}

	function ctxBulletList() {
		editor?.chain().focus().toggleBulletList().run();
		closeTextContextMenu();
	}

	function ctxOrderedList() {
		editor?.chain().focus().toggleOrderedList().run();
		closeTextContextMenu();
	}

	function ctxTaskList() {
		editor?.chain().focus().toggleTaskList().run();
		closeTextContextMenu();
	}

	// ── AI Actions ──

	function openAiMenu() {
		if (!editor) return;
		closeTextContextMenu();
		editor.commands.focus();

		const { from, to } = editor.state.selection;
		const hasSelection = from !== to;

		if (hasSelection) {
			const selectedText = editor.state.doc.textBetween(from, to, '\n');
			if (!selectedText.trim()) return;
			aiSelectionFrom = from;
			aiSelectionTo = to;
			aiSelectedText = selectedText;
			aiWholeNote = false;
		} else {
			const fullMarkdown = editorToMarkdown();
			if (!fullMarkdown.trim()) {
				aiEmptyNote = true;
				aiWholeNote = true;
				aiSelectedText = '';
				aiSelectionFrom = 0;
				aiSelectionTo = 0;
			} else {
				aiEmptyNote = false;
				aiSelectionFrom = 0;
				aiSelectionTo = editor.state.doc.content.size;
				aiOriginalMarkdown = fullMarkdown;

				// Replace images, PDF embeds, and HTML tags with placeholders so AI doesn't mangle them
				const placeholders = new Map<string, string>();
				let idx = 0;
				let textForAi = fullMarkdown;
				// Images: ![alt](src)
				textForAi = textForAi.replace(/!\[[^\]]*\]\([^)]*\)/g, (match) => {
					const key = `__MEDIA_${idx++}__`;
					placeholders.set(key, match);
					return key;
				});
				// PDF embeds
				textForAi = textForAi.replace(/<div[^>]*class="pdf-embed"[^>]*>[\s\S]*?<\/div>/gi, (match) => {
					const key = `__MEDIA_${idx++}__`;
					placeholders.set(key, match);
					return key;
				});
				// Inline HTML img tags
				textForAi = textForAi.replace(/<img[^>]*\/?>/gi, (match) => {
					const key = `__MEDIA_${idx++}__`;
					placeholders.set(key, match);
					return key;
				});
				aiMediaPlaceholders = placeholders;
				aiSelectedText = textForAi.trim();
				if (!aiSelectedText) return;
				aiWholeNote = true;
			}
		}

		aiResult = null;
		aiError = null;
		aiLoading = false;
		aiShowCustom = false;
		aiTranslateMenu = false;
		aiCustomPrompt = '';

		if (isMobile) {
			// Mobile: bottom sheet, no positioning needed
			aiMenu = { x: 0, y: 0 };
		} else if (hasSelection) {
			const coords = editor.view.coordsAtPos(from);
			let x = coords.left;
			let y = coords.top - 8;
			const menuWidth = 220;
			const menuHeight = 400;
			if (x + menuWidth > window.innerWidth) x = window.innerWidth - menuWidth - 8;
			if (y - menuHeight < 0) y = coords.bottom + 8;
			else y = y - menuHeight;
			if (y < 4) y = 4;
			aiMenu = { x, y };
		} else {
			// Center the menu in the editor area
			const editorRect = editorElement?.getBoundingClientRect();
			const menuWidth = 220;
			const menuHeight = 400;
			let x = editorRect ? editorRect.left + (editorRect.width - menuWidth) / 2 : window.innerWidth / 2 - menuWidth / 2;
			let y = editorRect ? editorRect.top + 60 : 100;
			if (x < 4) x = 4;
			if (y < 4) y = 4;
			aiMenu = { x, y };
		}
	}

	function closeAiMenu() {
		if (aiStreamUnlisten) { aiStreamUnlisten(); aiStreamUnlisten = null; }
		aiMenu = null;
		aiLoading = false;
		aiResult = null;
		aiError = null;
		aiShowCustom = false;
		aiTranslateMenu = false;
		aiWholeNote = false;
		aiEmptyNote = false;
		aiOriginalMarkdown = '';
		aiMediaPlaceholders = new Map();
	}

	async function runAiAction(action: string, customPrompt?: string) {
		if (!editor) return;
		if (!aiEmptyNote && !aiSelectedText.trim()) return;
		// For empty notes, force action to 'custom' with a generate instruction
		if (aiEmptyNote) {
			action = 'custom';
			customPrompt = `Generate a note based on this prompt. Start your response with a title on the first line (no # prefix), then a blank line, then the content in markdown. Prompt: ${customPrompt || aiCustomPrompt}`;
			aiSelectedText = '(empty note)';
		}
		aiLoading = true;
		aiResult = '';
		aiError = null;
		aiShowCustom = false;
		aiTranslateMenu = false;

		// Cancel any previous stream listener
		if (aiStreamUnlisten) { aiStreamUnlisten(); aiStreamUnlisten = null; }

		const requestId = crypto.randomUUID();
		const unlisten = await listen<AiStreamEvent>('ai-stream', (event) => {
			const data = event.payload;
			if (data.event_type === 'text' && data.text) {
				aiResult = (aiResult ?? '') + data.text;
			} else if (data.event_type === 'done') {
				aiLoading = false;
				aiStreamUnlisten = null;
				unlisten();
			} else if (data.event_type === 'error') {
				aiError = data.error ?? 'Unknown error';
				aiLoading = false;
				aiStreamUnlisten = null;
				unlisten();
			}
		});
		aiStreamUnlisten = unlisten;

		try {
			await aiAsk(action, aiSelectedText, customPrompt ?? null, requestId);
		} catch (e) {
			aiError = String(e);
			aiLoading = false;
			unlisten();
			aiStreamUnlisten = null;
		}
	}

	async function aiApplyResult() {
		if (!editor || !aiResult) return;
		// Save a version snapshot before applying AI changes
		if ($activeNotePath && $activeNote && !aiEmptyNote) {
			try {
				await forceSave();
				await createVersion($activeNotePath, $activeNote.meta.id);
			} catch (e) {
				console.error('Failed to create version before AI apply:', e);
			}
		}
		if (aiEmptyNote) {
			// Parse title from first line, rest is content
			const lines = aiResult.split('\n');
			let title = lines[0]?.replace(/^#+\s*/, '').trim() || 'Untitled';
			const body = lines.slice(1).join('\n').replace(/^\n+/, '');
			if ($activeNote) {
				$activeNote.meta.title = title;
			}
			editor.commands.setContent(markdownToHtml(body));
		} else if (aiWholeNote) {
			// Restore media placeholders back to original markdown
			let finalMarkdown = aiResult;
			for (const [key, original] of aiMediaPlaceholders) {
				finalMarkdown = finalMarkdown.replace(key, original);
			}
			// Replace entire document - convert markdown back to HTML for TipTap
			editor.commands.setContent(markdownToHtml(finalMarkdown));
		} else {
			// Replace the selected range with the AI result (convert markdown → HTML so TipTap renders it properly)
			const html = markdownToHtml(aiResult);
			editor.chain().focus()
				.setTextSelection({ from: aiSelectionFrom, to: aiSelectionTo })
				.deleteSelection()
				.insertContent(html)
				.run();
		}
		$editorDirty = true;
		autoSave();
		closeAiMenu();
	}

	function aiDiscard() {
		closeAiMenu();
		editor?.commands.focus();
	}

	function closeLinkContextMenu() {
		linkContextMenu = null;
	}

	/** Resolve a link href to an absolute .md note path, or null if not a note link. */
	function resolveNoteHref(href: string): string | null {
		const decoded = decodeURIComponent(href);
		if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(decoded)) return null;
		let absPath = decoded;
		if (!decoded.startsWith('/')) {
			const notePath = $activeNotePath;
			if (notePath) {
				const noteDir = notePath.substring(0, notePath.lastIndexOf('/'));
				absPath = normalizePath(`${noteDir}/${decoded}`);
			} else {
				const vaultRoot = $appConfig?.active_vault;
				if (vaultRoot) absPath = normalizePath(`${vaultRoot}/${decoded}`);
			}
		}
		return absPath.endsWith('.md') ? absPath : null;
	}

	function linkMenuOpen() {
		if (!linkContextMenu) return;
		const href = linkContextMenu.href;
		closeLinkContextMenu();
		// Internal .md note link - navigate within the app
		const notePath = resolveNoteHref(href);
		if (notePath) {
			navigateToWikiLink(notePath, '');
			return;
		}
		if (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:')) {
			openUrl(href).catch(console.error);
		} else {
			openFile(resolveHrefToAbsPath(href)).catch(console.error);
		}
	}

	function linkMenuCopy() {
		if (!linkContextMenu) return;
		navigator.clipboard.writeText(linkContextMenu.href).catch(console.error);
		closeLinkContextMenu();
	}

	function linkMenuEdit() {
		if (!linkContextMenu || !editor) return;
		const anchor = linkContextMenu.anchor;
		const href = linkContextMenu.href;
		closeLinkContextMenu();
		// Select the link text so the modal edits the right link
		const pos = editor.view.posAtDOM(anchor, 0);
		if (pos >= 0) {
			editor.chain().focus().setTextSelection(pos).extendMarkRange('link').run();
		}
		const { from, to } = editor.state.selection;
		linkSelectionFrom = from;
		linkSelectionTo = to;
		linkModalUrl = decodeURIComponent(href);
		linkModalDisplayText = '';
		linkModal = true;
		getAllNoteTitles().then(t => { linkSuggestTitles = t; }).catch(() => {});
		tick().then(() => linkModalInput?.focus());
	}

	function linkMenuRemove() {
		if (!linkContextMenu || !editor) return;
		const anchor = linkContextMenu.anchor;
		const pos = editor.view.posAtDOM(anchor, 0);
		if (pos >= 0) {
			editor.chain()
				.focus()
				.setTextSelection(pos)
				.extendMarkRange('link')
				.unsetLink()
				.run();
			$editorDirty = true;
			autoSave();
		}
		closeLinkContextMenu();
	}

	function resolveHrefToAbsPath(href: string): string {
		const decoded = decodeURIComponent(href);
		if (decoded.startsWith('/')) return decoded;
		// .helixnotes/ paths are always relative to vault root, not the note's directory
		const vaultRoot = $appConfig?.active_vault;
		if (decoded.startsWith('.helixnotes/') && vaultRoot) {
			return normalizePath(`${vaultRoot}/${decoded}`);
		}
		const notePath = $activeNotePath;
		if (notePath) {
			const noteDir = notePath.substring(0, notePath.lastIndexOf('/'));
			return normalizePath(`${noteDir}/${decoded}`);
		}
		if (vaultRoot) return normalizePath(`${vaultRoot}/${decoded}`);
		return decoded;
	}

	function isFileLink(href: string): boolean {
		if (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:')) return false;
		const ext = href.split('.').pop()?.toLowerCase() ?? '';
		return ['pdf', 'zip', 'rar', '7z', 'tar', 'gz', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'csv', 'txt', 'rtf', 'odt', 'ods', 'mp3', 'mp4', 'wav', 'avi', 'mov', 'mkv', 'exe', 'dmg', 'apk', 'iso', 'epub'].includes(ext);
	}

	async function linkMenuSaveAs() {
		if (!linkContextMenu) return;
		const href = linkContextMenu.href;
		closeLinkContextMenu();
		const absPath = resolveHrefToAbsPath(href);
		const filename = absPath.split('/').pop() || 'file';
		const dest = await saveDialog({ defaultPath: filename });
		if (dest) {
			try {
				await copyFileTo(absPath, dest);
			} catch (e) {
				console.error('Failed to save file:', e);
			}
		}
	}

	function handleFileDrop(event: DragEvent): boolean {
		const files = event.dataTransfer?.files;
		if (!files || files.length === 0) return false;
		event.preventDefault();
		for (const file of Array.from(files)) {
			if (file.type.startsWith('image/')) {
				insertImage(file);
			} else if (file.type === 'application/pdf') {
				insertPdf(file);
			} else {
				insertFileAttachment(file);
			}
		}
		return true;
	}

	function handleFilePaste(event: ClipboardEvent): boolean {
		const items = event.clipboardData?.items;
		if (!items) return false;
		for (const item of Array.from(items)) {
			const file = item.getAsFile();
			if (!file) continue;
			event.preventDefault();
			if (item.type.startsWith('image/')) {
				insertImage(file);
			} else if (file.type === 'application/pdf') {
				insertPdf(file);
			} else {
				insertFileAttachment(file);
			}
			return true;
		}
		// WebKitGTK fallback (bug #218519): older WebKitGTK versions return
		// empty DataTransferItemList for image pastes. Detect this and read
		// the image directly from the system clipboard via Rust/arboard.
		if (items.length === 0) {
			const hasText = event.clipboardData!.getData('text/plain');
			const hasHtml = event.clipboardData!.getData('text/html');
			if (!hasText && !hasHtml) {
				event.preventDefault();
				insertClipboardImage();
				return true;
			}
		}
		return false;
	}

	async function insertClipboardImage() {
		try {
			const data = await readClipboardImage();
			const relativePath = await saveImage('pasted-image.png', data);
			if (editor) {
				const displaySrc = resolveImageSrc(relativePath);
				editor.chain().focus().setImage({ src: displaySrc }).run();
			}
		} catch (e) {
			console.error('Clipboard image fallback failed:', e);
		}
	}

	async function insertImage(file: File) {
		try {
			const buffer = await file.arrayBuffer();
			const data = Array.from(new Uint8Array(buffer));
			const relativePath = await saveImage(file.name, data);
			if (editor) {
				const displaySrc = resolveImageSrc(relativePath);
				editor.chain().focus().setImage({ src: displaySrc }).run();
			}
		} catch (e) {
			console.error('Failed to insert image:', e);
		}
	}

	async function insertPdf(file: File) {
		try {
			const buffer = await file.arrayBuffer();
			const data = Array.from(new Uint8Array(buffer));
			const relativePath = await saveAttachment(file.name, data);
			if (!editor) return;
			const usePdfPreview = !isMobile && ($appConfig?.pdf_preview ?? false);
			if (usePdfPreview) {
				editor.chain().focus().insertContent({
					type: 'pdfEmbed',
					attrs: { src: relativePath, name: file.name },
				}).run();
			} else {
				const sizeKB = Math.round(file.size / 1024);
				const label = `${file.name} (${sizeKB} kB)`;
				editor.chain().focus()
					.insertContent(`<a href="${relativePath}">${label}</a> `)
					.run();
			}
		} catch (e) {
			console.error('Failed to insert PDF:', e);
		}
	}

	async function saveBlobImage(blobUrl: string): Promise<string | null> {
		try {
			const resp = await fetch(blobUrl);
			const blob = await resp.blob();
			const ext = blob.type.split('/')[1] || 'png';
			const name = `pasted-image.${ext}`;
			const buffer = await blob.arrayBuffer();
			const data = Array.from(new Uint8Array(buffer));
			const relativePath = await saveImage(name, data);
			return resolveImageSrc(relativePath);
		} catch (e) {
			console.error('Failed to save blob image:', e);
			return null;
		}
	}

	async function fixBlobImages() {
		if (!editor) return;
		const { doc, tr } = editor.state;
		let changed = false;
		const promises: Array<{ pos: number; blobUrl: string }> = [];
		doc.descendants((node, pos) => {
			if (node.type.name === 'image' && node.attrs.src?.startsWith('blob:')) {
				promises.push({ pos, blobUrl: node.attrs.src });
			}
		});
		for (const { pos, blobUrl } of promises) {
			const savedSrc = await saveBlobImage(blobUrl);
			if (savedSrc && editor) {
				const currentTr = editor.state.tr;
				// Re-find the node since positions may have shifted
				let found = false;
				editor.state.doc.descendants((node, nodePos) => {
					if (!found && node.type.name === 'image' && node.attrs.src === blobUrl) {
						currentTr.setNodeAttribute(nodePos, 'src', savedSrc);
						found = true;
						changed = true;
					}
				});
				if (found) editor.view.dispatch(currentTr);
			}
		}
		if (changed) {
			$editorDirty = true;
			autoSave();
		}
	}

	async function insertFileAttachment(file: File) {
		try {
			const buffer = await file.arrayBuffer();
			const data = Array.from(new Uint8Array(buffer));
			const relativePath = await saveAttachment(file.name, data);
			if (editor) {
				const sizeKB = Math.round(file.size / 1024);
				const label = `${file.name} (${sizeKB} kB)`;
				editor.chain().focus()
					.insertContent(`<a href="${relativePath}">${label}</a> `)
					.run();
			}
		} catch (e) {
			console.error('Failed to insert attachment:', e);
		}
	}

	// Source mode toggle - only react to explicit user toggle, not note switches
	$effect(() => {
		const isSource = $sourceMode;
		// Only act if we have a loaded note
		if (!loadedPath) return;

		if (isSource && !lastSourceMode) {
			// Switching TO source: extract markdown, then drop the caret on the same word (#125).
			const caretNonWs = editor
				? editor.state.doc.textBetween(0, editor.state.selection.from, '\n', '').replace(/\s/g, '').length
				: 0;
			const docNonWs = docNonWhitespace();
			sourceContent = editor ? editorToMarkdown() : ($activeNote?.content ?? '');
			resetSourceHistory(sourceContent);
			lastSourceMode = true;
			const target = caretNonWs > 0 ? scanAlign(sourceContent, docNonWs, { stopAtNw: caretNonWs }).srcOffset : 0;
			tick().then(() => {
				if (!sourceElement) return;
				if (!isMobile) sourceElement.focus();
				sourceElement.setSelectionRange(target, target);
				scrollSourceToOffset(target);
			});
		} else if (!isSource && lastSourceMode) {
			lastSourceMode = false;
			// Capture the source caret before the textarea is torn down (#125).
			const srcOffset = sourceElement ? sourceElement.selectionStart : 0;
			const srcText = sourceContent;
			// Re-anchor the caret in the rich editor once it holds the new content.
			const restoreRichCaret = () => {
				if (!editor) return;
				const nwBefore = scanAlign(srcText, docNonWhitespace(), { limit: srcOffset }).nwCount;
				const pos = Math.min(docPosForNonWsCount(editor.state.doc, nwBefore), editor.state.doc.content.size);
				const sel = TextSelection.near(editor.state.doc.resolve(pos));
				editor.view.dispatch(editor.state.tr.setSelection(sel).scrollIntoView());
				if (!isMobile) editor.view.focus();
				requestAnimationFrame(() => editor?.commands.scrollIntoView());
			};
			if (isMobile) {
				// Mobile: editor stays in DOM, just update its content
				const content = srcText || ($activeNote?.content ?? '');
				if (editor) {
					ignoreNextUpdate = true;
					editor.commands.setContent(markdownToHtml(content));
					tick().then(restoreRichCaret);
				}
			} else {
				// Desktop: destroy old editor (its DOM element is gone),
				// wait for DOM to swap textarea→div, then create editor on new element.
				destroyEditor();
				const content = srcText || ($activeNote?.content ?? '');
				tick().then(() => {
					if (editorElement && !editor) {
						createEditor(content);
						restoreRichCaret();
					}
				});
			}
		}
	});

	// Tauri drag-drop listener for OS file drops (browser DragEvent doesn't have files in Tauri)
	let unlistenDragDrop: (() => void) | null = null;
	$effect(() => {
		const appWindow = getCurrentWindow();
		appWindow.onDragDropEvent((event) => {
			if (event.payload.type !== 'drop' || !editor || !$activeNote) return;
			const paths = event.payload.paths;
			for (const filePath of paths) {
				const name = filePath.split('/').pop() || 'file';
				const ext = name.split('.').pop()?.toLowerCase() || '';
				if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) {
					readFile(filePath).then((data) => {
						saveImage(name, Array.from(data)).then((relativePath) => {
							if (editor) {
								editor.chain().focus().setImage({ src: resolveImageSrc(relativePath) }).run();
							}
						});
					}).catch((e) => console.error('Failed to drop image:', e));
				} else if (ext === 'pdf') {
					readFile(filePath).then((data) => {
						saveAttachment(name, Array.from(data)).then((relativePath) => {
							if (!editor) return;
							const usePdfPreview = !isMobile && ($appConfig?.pdf_preview ?? false);
							if (usePdfPreview) {
								editor.chain().focus().insertContent({
									type: 'pdfEmbed',
									attrs: { src: relativePath, name },
								}).run();
							} else {
								editor.chain().focus().insertContent(`<a href="${relativePath}">${name}</a> `).run();
							}
						});
					}).catch((e) => console.error('Failed to drop PDF:', e));
				} else {
					readFile(filePath).then((data) => {
						saveAttachment(name, Array.from(data)).then((relativePath) => {
							if (editor) {
								editor.chain().focus().insertContent(`<a href="${relativePath}">${name}</a> `).run();
							}
						});
					}).catch((e) => console.error('Failed to drop file:', e));
				}
			}
		}).then((unlisten) => {
			unlistenDragDrop = unlisten;
		});
		return () => {
			unlistenDragDrop?.();
		};
	});

	// Refresh wiki-link title cache whenever vault files change so that deleted
	// notes become unresolved and newly created notes become resolvable.
	let unlistenFileChange: (() => void) | null = null;
	if ($appConfig?.enable_wiki_links) {
		listen('file-changed', () => {
			if ($appConfig?.enable_wiki_links) refreshWikiLinkTitles();
		}).then(fn => { unlistenFileChange = fn; });
	}

	onDestroy(() => {
		destroyEditor();
		unlistenFileChange?.();
	});
</script>

<div class="editor-container" class:mobile={isMobile}>
	{#if !$activeNote}
		<div class="empty-editor">
			<div class="empty-icon">
				<svg width="48" height="48" viewBox="0 0 48 48" fill="none">
					<rect x="8" y="6" width="32" height="36" rx="4" stroke="var(--text-tertiary)" stroke-width="2" fill="none" />
					<path d="M16 16h16M16 22h12M16 28h16M16 34h8" stroke="var(--text-tertiary)" stroke-width="1.5" stroke-linecap="round" />
				</svg>
			</div>
			<p>Select a note or create a new one</p>
			<div class="shortcuts-hint">
				<span><kbd>{modKey}</kbd>+<kbd>N</kbd> New note</span>
				<span><kbd>{modKey}</kbd>+<kbd>P</kbd> Quick open</span>
			</div>
		</div>
	{:else}
		{#if $viewerNote}
			<div class="viewer-banner">
				<svg class="viewer-banner-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
				</svg>
				<span class="viewer-banner-label">Read-only viewer</span>
				<code class="viewer-banner-path" title={$viewerNote.path}>{$viewerNote.path}</code>
				<div class="viewer-banner-actions">
					<button type="button" class="viewer-banner-btn primary" onclick={() => (viewerImportPickerOpen = true)} disabled={viewerImportBusy || !$appConfig?.active_vault}>Import to vault…</button>
					<button type="button" class="viewer-banner-btn" onclick={closeViewer}>Close</button>
				</div>
				{#if viewerToast}
					<span class="viewer-banner-toast">{viewerToast}</span>
				{/if}
			</div>
		{/if}
		{#if !$viewerNote}
		<div class="editor-toolbar" class:mobile={isMobile}>
			<div class="editor-title">
				<input
					bind:this={titleInput}
					type="text"
					readonly={$readOnly}
					value={$activeNote.meta.title}
					onkeydown={(e) => {
						if (e.key === 'Tab') {
							e.preventDefault();
							editor?.commands.focus('start');
						}
						if (e.key === 'Enter') {
							e.preventDefault();
							editor?.commands.focus('start');
						}
					}}
					onchange={async (e) => {
						if ($activeNote && $activeNotePath) {
							const newTitle = (e.target as HTMLInputElement).value.trim();
							if (!newTitle) return;
							const oldPath = $activeNotePath;
							$activeNote.meta.title = newTitle;
							// Update stripped title so restoreTitleH1 uses the new title
							if (titleWasStripped) strippedTitle = newTitle;
							$editorDirty = true;
							// Force save current editor content before renaming so disk is up-to-date
							await forceSave();
							// Rename file on disk if filename doesn't match the new title
							const filename = oldPath.split('/').pop() ?? '';
							const stem = filename.replace(/\.md$/, '');
							if (stem !== newTitle) {
								try {
									const newPath = await renameNote(oldPath, newTitle);
									loadedPath = newPath;
									$activeNotePath = newPath;
									notes.update(list => list.map(n =>
										n.path === oldPath
											? { ...n, path: newPath, relative_path: n.relative_path.replace(/[^/]+$/, newTitle + '.md'), meta: { ...n.meta, title: newTitle } }
											: n
									));
									// Refresh wiki-link titles cache so links resolve to renamed note
									refreshWikiLinkTitles();
								} catch (err) {
									console.error('Failed to rename note file:', err);
									notes.update(list => list.map(n =>
										n.path === oldPath ? { ...n, meta: { ...n.meta, title: newTitle } } : n
									));
								}
							} else {
								notes.update(list => list.map(n =>
									n.path === oldPath ? { ...n, meta: { ...n.meta, title: newTitle } } : n
								));
							}
						}
					}}
				/>
			</div>
			{#if !isMobile}
			<div class="toolbar-actions">
				{#if $canGoBack || $canGoForward}
				<div class="nav-history-btns">
					<button class="nav-history-btn" disabled={!$canGoBack} onclick={() => editorNavigateHistory(-1)} title="Back (Alt+←)">
						<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
					</button>
					<button class="nav-history-btn" disabled={!$canGoForward} onclick={() => editorNavigateHistory(1)} title="Forward (Alt+→)">
						<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
					</button>
				</div>
				{/if}
				{#if $editorDirty}
					<span class="save-indicator">Unsaved</span>
				{/if}
				{#if $readOnly}
					<span class="readonly-indicator">View Mode</span>
				{/if}
				<button
					class="icon-btn"
					class:active={noteSearchOpen}
					onclick={() => noteSearchOpen ? closeNoteSearch() : openNoteSearch()}
					title={`Find in note (${modKey}+F)`}
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
					</svg>
				</button>

				<button
					class="icon-btn"
					class:active={!!tagMenu}
					onclick={toggleTagMenu}
					title="Edit tags"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/>
						<line x1="7" y1="7" x2="7.01" y2="7"/>
					</svg>
				</button>
				<button
					class="icon-btn"
					class:active={$activeNote?.meta.pinned}
					onclick={() => {
						if ($activeNote) {
							$activeNote.meta.pinned = !$activeNote.meta.pinned;
							$editorDirty = true;
							autoSave();
						}
					}}
					title={$activeNote?.meta.pinned ? 'Unpin note' : 'Pin note'}
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M12 17v5"/>
						<path d="M9 2h6l-1 7h4l-2 4H8l-2-4h4L9 2z"/>
					</svg>
				</button>
				<button
					class="icon-btn"
					class:active={isQuickAccess}
					onclick={async () => {
						if (!noteRelativePath) return;
						try {
							if (isQuickAccess) {
								await removeQuickAccess(noteRelativePath);
							} else {
								await addQuickAccess(noteRelativePath);
							}
							const qa = await getQuickAccess();
							$quickAccessPaths = qa.map(n => n.relative_path);
						} catch (e) {
							console.error('Quick access toggle failed:', e);
						}
					}}
					title={isQuickAccess ? 'Remove from Quick Access' : 'Add to Quick Access'}
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill={isQuickAccess ? 'currentColor' : 'none'} stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
					</svg>
				</button>
				<button
					class="icon-btn"
					class:active={showOutline}
					onclick={() => { showOutline = !showOutline; if (showOutline) updateOutline(); }}
					title="Outline"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<line x1="4" y1="6" x2="20" y2="6"/><line x1="8" y1="12" x2="20" y2="12"/><line x1="8" y1="18" x2="20" y2="18"/><circle cx="4" cy="12" r="1" fill="currentColor"/><circle cx="4" cy="18" r="1" fill="currentColor"/>
					</svg>
				</button>
				<button
					class="icon-btn"
					class:active={showHistory}
					onclick={toggleHistory}
					title="Version history"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<circle cx="12" cy="12" r="10"/>
						<polyline points="12 6 12 12 16 14"/>
					</svg>
				</button>
				<button
					bind:this={infoToggleBtnEl}
					class="icon-btn"
					class:active={showInfo}
					onclick={toggleInfo}
					title="Note info"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<circle cx="12" cy="12" r="10"/>
						<line x1="12" y1="8" x2="12" y2="8" stroke-width="3" stroke-linecap="round"/>
						<line x1="12" y1="12" x2="12" y2="16"/>
					</svg>
				</button>
				{#if $appConfig?.enable_wiki_links}
				<button
					class="icon-btn"
					class:active={showGraph}
					onclick={() => showGraph = !showGraph}
					title="Graph View"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/>
						<line x1="8.5" y1="7.5" x2="15.5" y2="16.5"/><line x1="15.5" y1="7.5" x2="8.5" y2="16.5"/>
					</svg>
				</button>
				{/if}
				{#if $appConfig?.ai_provider}
				<button
					class="icon-btn"
					onclick={openAiMenu}
					title="AI Actions"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M12 8V4l-2-2"/><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M9 13v2"/><path d="M15 13v2"/>
					</svg>
				</button>
				{/if}
				<button
					class="icon-btn"
					class:active={$sourceMode}
					onclick={() => ($sourceMode = !$sourceMode)}
					title="Toggle Markdown Editor"
				>
					<svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
						<path d="M5.854 4.854a.5.5 0 10-.708-.708l-3.5 3.5a.5.5 0 000 .708l3.5 3.5a.5.5 0 00.708-.708L2.707 8l3.147-3.146zm4.292 0a.5.5 0 01.708-.708l3.5 3.5a.5.5 0 010 .708l-3.5 3.5a.5.5 0 01-.708-.708L13.293 8l-3.147-3.146z" />
					</svg>
				</button>
			</div>
			{/if}
		</div>
		{/if}

		{#if !$focusMode}
		<div class="note-meta-bar">
			<span class="note-folder" class:unfiled={!noteFolder}>
				<svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
				{#if noteFolder}
					{#each noteFolder.split('/') as segment, i}
						{#if i > 0}<span class="path-sep">›</span>{/if}{segment}
					{/each}
				{:else}
					Unfiled Notes
				{/if}
			</span>
			<span class="meta-divider">·</span>
			<button class="note-tags-trigger" onclick={toggleTagMenu} title="Edit tags">
				{#if $activeNote.meta.tags?.length > 0}
					{#each $activeNote.meta.tags as tag}<span class="note-tag">#{tag}</span>{/each}
				{:else}
					<span class="note-tags-add">+ Tags</span>
				{/if}
			</button>
		</div>
		{/if}

		<div class="editor-body-wrapper">
			{#if noteSearchOpen}
				<div class="note-search-bar">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="opacity:0.5;flex-shrink:0"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
					<input
						bind:this={noteSearchInput}
						type="text"
						class="note-search-input"
						placeholder="Find in note..."
						bind:value={noteSearchQuery}
						oninput={() => updateNoteSearch(noteSearchQuery)}
						onkeydown={(e) => {
							if (e.key === 'Enter') { e.preventDefault(); e.shiftKey ? noteSearchPrev() : noteSearchNext(); }
							if (e.key === 'Escape') { e.preventDefault(); closeNoteSearch(); }
						}}
						use:autofocus
					/>
					<span class="note-search-count">
						{#if noteSearchQuery && noteSearchResults.length > 0}
							{noteSearchIndex + 1} / {noteSearchResults.length}
						{:else if noteSearchQuery}
							No results
						{/if}
					</span>
					<button class="note-search-btn" onclick={noteSearchPrev} title="Previous (Shift+Enter)">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="18 15 12 9 6 15"/></svg>
					</button>
					<button class="note-search-btn" onclick={noteSearchNext} title="Next (Enter)">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"/></svg>
					</button>
					<button class="note-search-btn" onclick={closeNoteSearch} title="Close (Esc)">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
					</button>
				</div>
			{/if}
			<div class="editor-body-row">
			<div class="editor-body">
				{#if isMobile}
					<!-- Mobile: both views always in DOM, toggled via display to avoid slow editor re-creation -->
					<textarea
						class="source-editor"
						style={$sourceMode ? '' : 'display:none'}
						bind:this={sourceElement}
						bind:value={sourceContent}
						readonly={$readOnly}
						oninput={() => {
							$editorDirty = true;
							autoSave();
							pushSourceHistoryDebounced();
						}}
						onkeydown={(e) => {
							if (handleSourceCtrlEnd(e)) return;
							if (handleSourceSelectionPair(e)) return;
							const mod = e.ctrlKey || e.metaKey;
							if (e.key === 'Enter' && e.shiftKey && !mod) {
								e.preventDefault();
								const ta = sourceElement;
								const start = ta.selectionStart;
								const end = ta.selectionEnd;
								const val = ta.value;
								sourceContent = val.slice(0, start) + '  \n' + val.slice(end);
								tick().then(() => {
									const newPos = start + 3;
									ta.setSelectionRange(newPos, newPos);
								});
								$editorDirty = true;
								autoSave();
								pushSourceHistoryDebounced();
								return;
							}
							if (mod && (e.key === 'z' || e.key === 'Z') && !e.shiftKey) {
								e.preventDefault();
								sourceUndo();
								return;
							}
							if ((mod && (e.key === 'z' || e.key === 'Z') && e.shiftKey) || (mod && (e.key === 'y' || e.key === 'Y'))) {
								e.preventDefault();
								sourceRedo();
								return;
							}
						}}
						spellcheck="false"
					></textarea>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="tiptap-wrapper" class:large-doc={isLargeDoc} style={$sourceMode ? 'display:none' : ''} spellcheck="false" bind:this={editorElement} onclick={(e) => { closeLinkContextMenu(); handleEditorClick(e); }}></div>
				{:else}
					<!-- Desktop: conditional rendering with line numbers -->
					{#if $sourceMode}
						{#if $appConfig?.show_line_numbers}
							<div class="line-numbers-clip" aria-hidden="true">
								<div class="line-numbers">
									{#each sourceContent.split('\n') as _, i}
										<span>{i + 1}</span>
									{/each}
								</div>
							</div>
						{/if}
						<textarea
							class="source-editor"
							class:with-line-numbers={$appConfig?.show_line_numbers}
							bind:this={sourceElement}
							bind:value={sourceContent}
							readonly={$readOnly}
							oninput={() => {
								$editorDirty = true;
								autoSave();
								pushSourceHistoryDebounced();
							}}
							onkeydown={(e) => {
								if (handleSourceCtrlEnd(e)) return;
								if (handleSourceSelectionPair(e)) return;
								const mod = e.ctrlKey || e.metaKey;
								// Shift+Enter: insert two trailing spaces + newline for markdown hard break
								if (e.key === 'Enter' && e.shiftKey && !mod) {
									e.preventDefault();
									const ta = sourceElement;
									const start = ta.selectionStart;
									const end = ta.selectionEnd;
									const val = ta.value;
									sourceContent = val.slice(0, start) + '  \n' + val.slice(end);
									tick().then(() => {
										const newPos = start + 3;
										ta.setSelectionRange(newPos, newPos);
									});
									$editorDirty = true;
									autoSave();
									pushSourceHistoryDebounced();
									return;
								}
								// Undo
								if (mod && (e.key === 'z' || e.key === 'Z') && !e.shiftKey) {
									e.preventDefault();
									sourceUndo();
									return;
								}
								// Redo
								if ((mod && (e.key === 'z' || e.key === 'Z') && e.shiftKey) || (mod && (e.key === 'y' || e.key === 'Y'))) {
									e.preventDefault();
									sourceRedo();
									return;
								}
								if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
									e.preventDefault();
									pushSourceHistoryImmediate();
									const ta = sourceElement;
									const val = ta.value;
									const start = ta.selectionStart;
									const lines = val.split('\n');
									// Find current line index
									let pos = 0;
									let curLine = 0;
									for (let i = 0; i < lines.length; i++) {
										if (pos + lines[i].length >= start) { curLine = i; break; }
										pos += lines[i].length + 1;
									}
									if (e.key === 'ArrowUp' && curLine > 0) {
										const tmp = lines[curLine];
										lines[curLine] = lines[curLine - 1];
										lines[curLine - 1] = tmp;
										sourceContent = lines.join('\n');
										tick().then(() => {
											const newPos = lines.slice(0, curLine - 1).join('\n').length + 1 + (start - pos);
											ta.setSelectionRange(newPos, newPos);
											pushSourceHistoryImmediate();
										});
									} else if (e.key === 'ArrowDown' && curLine < lines.length - 1) {
										const tmp = lines[curLine];
										lines[curLine] = lines[curLine + 1];
										lines[curLine + 1] = tmp;
										sourceContent = lines.join('\n');
										tick().then(() => {
											const newPos = lines.slice(0, curLine + 1).join('\n').length + 1 + (start - pos);
											ta.setSelectionRange(newPos, newPos);
											pushSourceHistoryImmediate();
										});
									}
									$editorDirty = true;
									autoSave();
								}
							}}
							onscroll={() => {
								if ($appConfig?.show_line_numbers) {
									const clip = sourceElement?.previousElementSibling as HTMLElement;
									const gutter = clip?.firstElementChild as HTMLElement;
									if (gutter) {
										gutter.style.transform = `translateY(-${sourceElement.scrollTop}px)`;
									}
								}
							}}
							spellcheck="false"
						></textarea>
					{:else}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="tiptap-wrapper" class:large-doc={isLargeDoc} spellcheck="false" bind:this={editorElement} onclick={(e) => { closeLinkContextMenu(); handleEditorClick(e); }} oncontextmenu={handleEditorContextMenu}></div>
					{/if}
				{/if}
			</div>

			{#if showHistory}
				<div class="history-panel">
					<div class="history-header">
						<h3>Version History</h3>
						<div class="history-header-actions">
							<button class="history-create-btn" onclick={handleCreateVersion} title="Save current state as a version">
								<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
									<line x1="12" y1="5" x2="12" y2="19" />
									<line x1="5" y1="12" x2="19" y2="12" />
								</svg>
							</button>
							<button class="history-close" onclick={() => { showHistory = false; historyPreview = null; historySelected = null; }}>
								<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
									<line x1="18" y1="6" x2="6" y2="18" />
									<line x1="6" y1="6" x2="18" y2="18" />
								</svg>
							</button>
						</div>
					</div>
					{#if historyLoading}
						<div class="history-empty">Loading...</div>
					{:else if historyVersions.length === 0}
						<div class="history-empty">No versions yet. Versions are created automatically as you edit (at least 5 minutes apart).</div>
					{:else}
						<div class="history-list">
							{#each historyVersions as v}
								<button
									class="history-item"
									class:active={historySelected?.timestamp === v.timestamp}
									onclick={() => previewVersion(v)}
								>
									<span class="history-date">{formatVersionDate(v.timestamp)}</span>
									<span class="history-size">{formatVersionSize(v.size)}</span>
								</button>
							{/each}
						</div>
					{/if}
					{#if historySelected}
						<div class="history-actions">
							<button class="history-restore-btn" onclick={restoreVersion}>
								Restore this version
							</button>
						</div>
					{/if}
				</div>
			{/if}
			{#if showOutline}
				<ResizeHandle onResize={handleOutlineResize} />
				<div class="outline-panel" style="width: {$outlineWidth}px">
					<div class="outline-header">
						<h3>Outline</h3>
						<button class="outline-close" onclick={() => { showOutline = false; }}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
								<line x1="18" y1="6" x2="6" y2="18" />
								<line x1="6" y1="6" x2="18" y2="18" />
							</svg>
						</button>
					</div>
					{#if outlineHeadings.length === 0}
						<div class="outline-empty">No headings in this note.</div>
					{:else}
						<div class="outline-list">
							{#each outlineHeadings as h}
								<button
									class="outline-item outline-level-{h.level}"
									onclick={() => scrollToHeading(h.pos)}
								>
									{h.text}
								</button>
							{/each}
						</div>
					{/if}
				</div>
			{/if}
			{#if showInfo && $activeNote}
				<div class="info-panel" bind:this={infoPanelEl}>
					<div class="info-panel-header">
						<span class="info-panel-title">Note Info</span>
						<button class="info-close-btn" onclick={() => showInfo = false}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
								<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
							</svg>
						</button>
					</div>

					<div class="info-section">
						<div class="info-section-label">Statistics</div>
						<div class="info-row">
							<span class="info-key">Words</span>
							<span class="info-value">{wordCount.toLocaleString()}</span>
						</div>
						<div class="info-row">
							<span class="info-key">Characters</span>
							<span class="info-value">{charCount.toLocaleString()}</span>
						</div>
						<div class="info-row">
							<span class="info-key">Reading time</span>
							<span class="info-value">{wordCount < 200 ? '< 1 min' : `${Math.ceil(wordCount / 200)} min`}</span>
						</div>
					</div>

					<div class="info-section">
						<div class="info-section-label">Details</div>
						<div class="info-row">
							<span class="info-key">Modified</span>
							<span class="info-value" title={$activeNote.meta.modified}>{formatVersionDate($activeNote.meta.modified)}</span>
						</div>
						<div class="info-row">
							<span class="info-key">Created</span>
							<span class="info-value" title={$activeNote.meta.created}>{formatVersionDate($activeNote.meta.created)}</span>
						</div>
						{#if noteFolder}
						<div class="info-row">
							<span class="info-key">Location</span>
							<span class="info-value info-value-path" title={noteFolder}>{noteFolder}</span>
						</div>
						{/if}
						{#if $activeNote.meta.tags?.length > 0}
						<div class="info-row info-row-tags">
							<span class="info-key">Tags</span>
							<span class="info-value info-tags">
								{#each $activeNote.meta.tags as tag}
									<span class="info-tag">#{tag}</span>
								{/each}
							</span>
						</div>
						{/if}
					</div>

					<div class="info-section info-section-versions">
						<div class="info-section-header">
							<div class="info-section-label">Snapshots</div>
							<button class="info-snapshot-btn" onclick={handleCreateVersion} title="Save snapshot">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
									<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
								</svg>
								Save now
							</button>
						</div>
						{#if historyLoading}
							<div class="info-empty">Loading...</div>
						{:else if historyVersions.length === 0}
							<div class="info-empty">No snapshots yet. They're created automatically as you edit.</div>
						{:else}
							<div class="info-versions-list">
								{#each historyVersions as v}
									<button
										class="info-version-item"
										class:active={historySelected?.timestamp === v.timestamp}
										onclick={() => { previewVersion(v); showHistory = true; showInfo = false; }}
									>
										<span class="info-version-date">{formatVersionDate(v.timestamp)}</span>
										<span class="info-version-size">{formatVersionSize(v.size)}</span>
									</button>
								{/each}
							</div>
						{/if}
					</div>
				</div>
			{/if}
			</div>
		</div>

		{#if editorReady && !$sourceMode && !$viewerNote}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="editor-formatting-bar" style={isMobile ? `${keyboardHeight > 0 ? `bottom: ${keyboardHeight}px;` : ''}${anyDropdownOpen ? 'overflow: visible;' : ''}` : ''} onclick={() => { headingDropdown = false; colorDropdown = false; highlightDropdown = false; tablePickerOpen = false; alignDropdown = false; insertDropdown = false; }}>
				{#if isMobile}
				<!-- ═══ MOBILE formatting bar: compact, relevant buttons only ═══ -->

				<!-- Insert (+) dropdown - at front like desktop -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn insert-btn" onclick={(e) => { e.stopPropagation(); insertDropdown = !insertDropdown; headingDropdown = false; }} title="Insert">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>
					</button>
					{#if insertDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown insert-dropdown" onclick={(e) => e.stopPropagation()}>
							<button onclick={() => { insertDropdown = false; document.querySelector<HTMLInputElement>('#insert-image-input')?.click(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 00-2.828 0L6 21"/></svg>
								Image
							</button>
							<button onclick={() => { insertDropdown = false; document.querySelector<HTMLInputElement>('#insert-file-input')?.click(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 22a2 2 0 01-2-2V4a2 2 0 012-2h8a2.4 2.4 0 011.704.706l3.588 3.588A2.4 2.4 0 0120 8v12a2 2 0 01-2 2z"/><path d="M14 2v5a1 1 0 001 1h5"/></svg>
								File
							</button>
							<button onclick={() => { insertDropdown = false; editor?.chain().focus().toggleCodeBlock().run(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m10 9-3 3 3 3"/><path d="m14 15 3-3-3-3"/><rect x="3" y="3" width="18" height="18" rx="2"/></svg>
								Code Block
							</button>
							<button onclick={() => { insertDropdown = false; editor?.chain().focus().toggleBlockquote().run(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 5H3"/><path d="M21 12H8"/><path d="M21 19H8"/><path d="M3 12v7"/></svg>
								Quote
							</button>
							<button onclick={() => { insertDropdown = false; tablePickerOpen = true; }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v18"/><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/></svg>
								Table
							</button>
							<button onclick={() => { insertDropdown = false; insertDetails(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="13" height="7" x="8" y="3" rx="1"/><path d="m2 9 3 3-3 3"/><rect width="13" height="7" x="8" y="14" rx="1"/></svg>
								Collapsible Section
							</button>
							<button onclick={() => { insertDropdown = false; insertCallout('note'); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2"/><line x1="7" y1="5" x2="7" y2="19"/></svg>
								Callout
							</button>
							<button onclick={() => { insertDropdown = false; editor?.chain().focus().setHorizontalRule().run(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14"/></svg>
								Horizontal Rule
							</button>
							<button onclick={() => { insertDropdown = false; openSecretInsert(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="10" rx="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
								Secret
							</button>
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Heading dropdown -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" class:active={(editorState, editor.isActive('heading'))} onclick={(e) => { e.stopPropagation(); headingDropdown = !headingDropdown; insertDropdown = false; }} title="Heading">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 12h12"/><path d="M6 20V4"/><path d="M18 20V4"/></svg>
					</button>
					{#if headingDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown" onclick={(e) => e.stopPropagation()}>
							<button class:active={(editorState, editor.isActive('heading', { level: 1 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 1 }).run(); headingDropdown = false; }}>Heading 1</button>
							<button class:active={(editorState, editor.isActive('heading', { level: 2 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 2 }).run(); headingDropdown = false; }}>Heading 2</button>
							<button class:active={(editorState, editor.isActive('heading', { level: 3 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 3 }).run(); headingDropdown = false; }}>Heading 3</button>
							<button class:active={(editorState, editor.isActive('paragraph'))} onclick={() => { editor?.chain().focus().setParagraph().run(); headingDropdown = false; }}>Paragraph</button>
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Bold / Italic / Underline / Strike -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('bold'))} onclick={() => editor?.chain().focus().toggleBold().run()} title="Bold">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 12h9a4 4 0 010 8H7a1 1 0 01-1-1V5a1 1 0 011-1h7a4 4 0 010 8"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('italic'))} onclick={() => editor?.chain().focus().toggleItalic().run()} title="Italic">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" x2="10" y1="4" y2="4"/><line x1="14" x2="5" y1="20" y2="20"/><line x1="15" x2="9" y1="4" y2="20"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('underline'))} onclick={() => editor?.chain().focus().toggleUnderline().run()} title="Underline">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4v6a6 6 0 0012 0V4"/><line x1="4" x2="20" y1="20" y2="20"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('strike'))} onclick={() => editor?.chain().focus().toggleStrike().run()} title="Strikethrough">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4H9a3 3 0 00-2.83 4"/><path d="M14 12a4 4 0 010 8H6"/><line x1="4" x2="20" y1="12" y2="12"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Link -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('link'))} onclick={addLinkFromToolbar} title="Link">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Lists -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('bulletList'))} onclick={() => editor?.chain().focus().toggleBulletList().run()} title="Bullet List">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('orderedList'))} onclick={() => editor?.chain().focus().toggleOrderedList().run()} title="Numbered List">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5h10"/><path d="M11 12h10"/><path d="M11 19h10"/><path d="M4 4h1v5"/><path d="M4 9h2"/><path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 00-2.6-1.02"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('taskList'))} onclick={() => editor?.chain().focus().toggleTaskList().run()} title="Task List">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 5h8"/><path d="M13 12h8"/><path d="M13 19h8"/><path d="m3 17 2 2 4-4"/><path d="m3 7 2 2 4-4"/></svg>
				</button>

				<!-- Indent / Outdent (no Tab key on mobile soft keyboards) -->
				<button class="fmt-btn" onclick={() => {
					if (!editor) return;
					// Try list indent first - run() returns true if it succeeded
					const sank = editor.chain().focus().sinkListItem('listItem').run();
					if (!sank) {
						const sankTask = editor.chain().focus().sinkListItem('taskItem').run();
						if (!sankTask && editor.state.selection.empty) {
							editor.chain().focus().insertContent('\t').run();
						}
					}
				}} title="Indent">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="4" x2="21" y2="4"/><line x1="11" y1="9" x2="21" y2="9"/><line x1="11" y1="14" x2="21" y2="14"/><line x1="3" y1="19" x2="21" y2="19"/><polyline points="3 9 7 11.5 3 14"/></svg>
				</button>
				<button class="fmt-btn" onclick={() => {
					if (!editor) return;
					const lifted = editor.chain().focus().liftListItem('listItem').run();
					if (!lifted) {
						const liftedTask = editor.chain().focus().liftListItem('taskItem').run();
						if (!liftedTask && editor.state.selection.empty) {
							// Remove leading tab/spaces from current line
							const { from } = editor.state.selection;
							const pos = editor.state.doc.resolve(from);
							const lineStart = pos.start(pos.depth);
							const lineText = editor.state.doc.textBetween(lineStart, pos.end(pos.depth));
							if (lineText.startsWith('\t')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 1);
									return true;
								}).run();
							} else if (lineText.startsWith('    ')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 4);
									return true;
								}).run();
							} else if (lineText.startsWith('  ')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 2);
									return true;
								}).run();
							}
						}
					}
				}} title="Outdent">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="4" x2="21" y2="4"/><line x1="11" y1="9" x2="21" y2="9"/><line x1="11" y1="14" x2="21" y2="14"/><line x1="3" y1="19" x2="21" y2="19"/><polyline points="7 9 3 11.5 7 14"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Highlight -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('highlight'))} onclick={() => editor?.chain().focus().toggleHighlight({ color: highlightColors[0].value }).run()} title="Highlight">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 01-2.8 0l-5.2-5.2a2 2 0 010-2.8L14 4"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Insert date/time -->
				<button class="fmt-btn" onclick={() => insertTimestamp('datetime')} title="Insert date and time">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h6"/><line x1="3" y1="10" x2="21" y2="10"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="16" y1="2" x2="16" y2="6"/><circle cx="18" cy="17" r="4"/><path d="M18 15.5v1.5l1 1"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Undo / Redo -->
				<button class="fmt-btn" onclick={() => editor?.chain().focus().undo().run()} title="Undo">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 015.5 5.5 5.5 5.5 0 01-5.5 5.5H11"/></svg>
				</button>
				<button class="fmt-btn" onclick={() => editor?.chain().focus().redo().run()} title="Redo">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 14 5-5-5-5"/><path d="M20 9H9.5A5.5 5.5 0 004 14.5 5.5 5.5 0 009.5 20H13"/></svg>
				</button>

				{#if $appConfig?.ai_provider}
				<div class="fmt-sep"></div>
				<button class="fmt-btn" onclick={openAiMenu} title="AI Actions">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M12 8V4l-2-2"/><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M9 13v2"/><path d="M15 13v2"/>
					</svg>
				</button>
				{/if}

				{:else}
				<!-- ═══ DESKTOP formatting bar: full feature set ═══ -->

				<!-- Insert (+) dropdown -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn insert-btn" onclick={(e) => { e.stopPropagation(); insertDropdown = !insertDropdown; headingDropdown = false; colorDropdown = false; highlightDropdown = false; tablePickerOpen = false; alignDropdown = false; }} title="Insert">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>
					</button>
					{#if insertDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown insert-dropdown" onclick={(e) => e.stopPropagation()}>
							<button onclick={() => { insertDropdown = false; document.querySelector<HTMLInputElement>('#insert-image-input')?.click(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 00-2.828 0L6 21"/></svg>
								Image
							</button>
							<button onclick={() => { insertDropdown = false; document.querySelector<HTMLInputElement>('#insert-file-input')?.click(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 22a2 2 0 01-2-2V4a2 2 0 012-2h8a2.4 2.4 0 011.704.706l3.588 3.588A2.4 2.4 0 0120 8v12a2 2 0 01-2 2z"/><path d="M14 2v5a1 1 0 001 1h5"/></svg>
								File
							</button>
							<button onclick={() => { insertDropdown = false; editor?.chain().focus().setHorizontalRule().run(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14"/></svg>
								Horizontal Rule
							</button>
							<button onclick={() => { insertDropdown = false; openSecretInsert(); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="10" rx="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
								Secret
							</button>
							<button onclick={() => { insertDropdown = false; insertTimestamp('datetime'); }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h6"/><line x1="3" y1="10" x2="21" y2="10"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="16" y1="2" x2="16" y2="6"/><circle cx="18" cy="17" r="4"/><path d="M18 15.5v1.5l1 1"/></svg>
								Date &amp; Time
							</button>
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Heading dropdown -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" class:active={(editorState, editor.isActive('heading'))} onclick={(e) => { e.stopPropagation(); headingDropdown = !headingDropdown; colorDropdown = false; highlightDropdown = false; tablePickerOpen = false; alignDropdown = false; insertDropdown = false; }} title="Heading">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 12h12"/><path d="M6 20V4"/><path d="M18 20V4"/></svg>
					</button>
					{#if headingDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown" onclick={(e) => e.stopPropagation()}>
							<button class:active={(editorState, editor.isActive('heading', { level: 1 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 1 }).run(); headingDropdown = false; }}>Heading 1</button>
							<button class:active={(editorState, editor.isActive('heading', { level: 2 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 2 }).run(); headingDropdown = false; }}>Heading 2</button>
							<button class:active={(editorState, editor.isActive('heading', { level: 3 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 3 }).run(); headingDropdown = false; }}>Heading 3</button>
							<button class:active={(editorState, editor.isActive('heading', { level: 4 }))} onclick={() => { editor?.chain().focus().toggleHeading({ level: 4 }).run(); headingDropdown = false; }}>Heading 4</button>
							<button class:active={(editorState, editor.isActive('paragraph'))} onclick={() => { editor?.chain().focus().setParagraph().run(); headingDropdown = false; }}>Paragraph</button>
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Text formatting -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('bold'))} onclick={() => editor?.chain().focus().toggleBold().run()} title={`Bold (${modKey}+B)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 12h9a4 4 0 010 8H7a1 1 0 01-1-1V5a1 1 0 011-1h7a4 4 0 010 8"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('italic'))} onclick={() => editor?.chain().focus().toggleItalic().run()} title={`Italic (${modKey}+I)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" x2="10" y1="4" y2="4"/><line x1="14" x2="5" y1="20" y2="20"/><line x1="15" x2="9" y1="4" y2="20"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('underline'))} onclick={() => editor?.chain().focus().toggleUnderline().run()} title={`Underline (${modKey}+U)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4v6a6 6 0 0012 0V4"/><line x1="4" x2="20" y1="20" y2="20"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('strike'))} onclick={() => editor?.chain().focus().toggleStrike().run()} title={`Strikethrough (${modKey}+Shift+X)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4H9a3 3 0 00-2.83 4"/><path d="M14 12a4 4 0 010 8H6"/><line x1="4" x2="20" y1="12" y2="12"/></svg>
				</button>

				<!-- Text color -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" onclick={(e) => { e.stopPropagation(); colorDropdown = !colorDropdown; headingDropdown = false; highlightDropdown = false; tablePickerOpen = false; alignDropdown = false; insertDropdown = false; }} title="Text Color">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 20h16"/><path d="m6 16 6-12 6 12"/><path d="M8 12h8"/></svg>
						<span class="color-indicator" style="background: {editor.getAttributes('textStyle').color || 'var(--accent)'}"></span>
					</button>
					{#if colorDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown color-grid-dropdown" onclick={(e) => e.stopPropagation()}>
							{#each textColors as color}
								<button class="color-swatch" title={color.name} onclick={() => setTextColor(color.value)} style="background: {color.value || 'var(--text-primary)'}">
									{#if (color.value === '' && !editor.getAttributes('textStyle').color) || editor.getAttributes('textStyle').color === color.value}
										<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
									{/if}
								</button>
							{/each}
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Link -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('link'))} onclick={addLinkFromToolbar} title={`Link (${modKey}+K)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Lists -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('bulletList'))} onclick={() => editor?.chain().focus().toggleBulletList().run()} title={`Bullet List (${modKey}+Shift+8)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h.01"/><path d="M3 12h.01"/><path d="M3 19h.01"/><path d="M8 5h13"/><path d="M8 12h13"/><path d="M8 19h13"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('orderedList'))} onclick={() => editor?.chain().focus().toggleOrderedList().run()} title={`Ordered List (${modKey}+Shift+7)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5h10"/><path d="M11 12h10"/><path d="M11 19h10"/><path d="M4 4h1v5"/><path d="M4 9h2"/><path d="M6.5 20H3.4c0-1 2.6-1.925 2.6-3.5a1.5 1.5 0 00-2.6-1.02"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('taskList'))} onclick={() => editor?.chain().focus().toggleTaskList().run()} title={`Task List (${modKey}+Shift+9)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 5h8"/><path d="M13 12h8"/><path d="M13 19h8"/><path d="m3 17 2 2 4-4"/><path d="m3 7 2 2 4-4"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Undo / Redo -->
				<button class="fmt-btn" onclick={() => editor?.chain().focus().undo().run()} title={`Undo (${modKey}+Z)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 015.5 5.5 5.5 5.5 0 01-5.5 5.5H11"/></svg>
				</button>
				<button class="fmt-btn" onclick={() => editor?.chain().focus().redo().run()} title={`Redo (${modKey}+Shift+Z)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 14 5-5-5-5"/><path d="M20 9H9.5A5.5 5.5 0 004 14.5 5.5 5.5 0 009.5 20H13"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Code & Code Block -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('code'))} onclick={() => editor?.chain().focus().toggleCode().run()} title={`Inline Code (${modKey}+E)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('codeBlock'))} onclick={() => editor?.chain().focus().toggleCodeBlock().run()} title={`Code Block (${modKey}+Alt+C)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m10 9-3 3 3 3"/><path d="m14 15 3-3-3-3"/><rect x="3" y="3" width="18" height="18" rx="2"/></svg>
				</button>

				<!-- Blockquote -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('blockquote'))} onclick={() => editor?.chain().focus().toggleBlockquote().run()} title={`Quote (${modKey}+Shift+B)`}>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17 5H3"/><path d="M21 12H8"/><path d="M21 19H8"/><path d="M3 12v7"/></svg>
				</button>

				<!-- Collapsible Section -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('details'))} onclick={() => insertDetails()} title="Collapsible Section">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="13" height="7" x="8" y="3" rx="1"/><path d="m2 9 3 3-3 3"/><rect width="13" height="7" x="8" y="14" rx="1"/></svg>
				</button>

				<!-- Callout -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('callout'))} onclick={() => insertCallout('note')} title="Callout">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2"/><line x1="7" y1="5" x2="7" y2="19"/></svg>
				</button>

				<!-- Table -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" onclick={(e) => { e.stopPropagation(); tablePickerOpen = !tablePickerOpen; headingDropdown = false; colorDropdown = false; highlightDropdown = false; alignDropdown = false; insertDropdown = false; }} title="Insert Table">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v18"/><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/></svg>
					</button>
					{#if tablePickerOpen}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown table-picker-dropdown" onclick={(e) => e.stopPropagation()}>
							<div class="table-picker-grid">
								{#each Array(8) as _, r}
									{#each Array(10) as _, c}
										<!-- svelte-ignore a11y_no_static_element_interactions -->
										<div
											class="table-picker-cell"
											class:active={r < tablePickerHover.rows && c < tablePickerHover.cols}
											onmouseenter={() => tablePickerHover = { rows: r + 1, cols: c + 1 }}
											onclick={() => insertTable(r + 1, c + 1)}
										></div>
									{/each}
								{/each}
							</div>
							<div class="table-picker-label">
								{tablePickerHover.rows > 0 ? `${tablePickerHover.rows} x ${tablePickerHover.cols}` : 'Select size'}
							</div>
						</div>
					{/if}
				</div>

				<!-- Horizontal Rule -->
				<button class="fmt-btn" onclick={() => editor?.chain().focus().setHorizontalRule().run()} title="Horizontal Rule">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M5 12h14"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Highlight -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" class:active={(editorState, editor.isActive('highlight'))} onclick={(e) => { e.stopPropagation(); highlightDropdown = !highlightDropdown; headingDropdown = false; colorDropdown = false; tablePickerOpen = false; alignDropdown = false; insertDropdown = false; }} title={`Highlight (${modKey}+Shift+H)`}>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 01-2.8 0l-5.2-5.2a2 2 0 010-2.8L14 4"/></svg>
						<span class="color-indicator" style="background: {editor.getAttributes('highlight').color || 'var(--accent)'}"></span>
					</button>
					{#if highlightDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown color-grid-dropdown" onclick={(e) => e.stopPropagation()}>
							{#each highlightColors as color}
								<button class="color-swatch" title={color.name} onclick={() => setHighlightColor(color.value)} style="background: {color.swatch}">
									{#if editor.isActive('highlight', { color: color.value })}
										<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
									{/if}
								</button>
							{/each}
							<button class="color-swatch" title="Remove highlight" onclick={() => setHighlightColor('')} style="background: var(--bg-tertiary);">
								{#if !editor.isActive('highlight')}
									<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--text-primary)" stroke-width="3"><polyline points="20 6 9 17 4 12"/></svg>
								{:else}
									<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="var(--text-tertiary)" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
								{/if}
							</button>
						</div>
					{/if}
				</div>

				<!-- Subscript & Superscript -->
				<button class="fmt-btn" class:active={(editorState, editor.isActive('subscript'))} onclick={() => editor?.chain().focus().toggleSubscript().run()} title="Subscript">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m4 5 8 8"/><path d="m12 5-8 8"/><path d="M20 19h-4c0-1.5.44-2 1.5-2.5S20 15.33 20 14c0-.47-.17-.93-.48-1.29a2.11 2.11 0 00-2.62-.44c-.42.24-.74.62-.9 1.07"/></svg>
				</button>
				<button class="fmt-btn" class:active={(editorState, editor.isActive('superscript'))} onclick={() => editor?.chain().focus().toggleSuperscript().run()} title="Superscript">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m4 19 8-8"/><path d="m12 19-8-8"/><path d="M20 12h-4c0-1.5.442-2 1.5-2.5S20 8.334 20 7.002c0-.472-.17-.93-.484-1.29a2.105 2.105 0 00-2.617-.436c-.42.239-.738.614-.899 1.06"/></svg>
				</button>

				<div class="fmt-sep"></div>

				<!-- Text Alignment -->
				<div class="fmt-dropdown-wrap">
					<button class="fmt-btn" onclick={(e) => { e.stopPropagation(); alignDropdown = !alignDropdown; headingDropdown = false; colorDropdown = false; highlightDropdown = false; tablePickerOpen = false; insertDropdown = false; }} title="Text Alignment">
						{#if (editorState, editor.isActive({ textAlign: 'center' }))}
							<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M17 12H7"/><path d="M19 19H5"/></svg>
						{:else if (editorState, editor.isActive({ textAlign: 'right' }))}
							<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M21 12H9"/><path d="M21 19H7"/></svg>
						{:else if (editorState, editor.isActive({ textAlign: 'justify' }))}
							<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h18"/><path d="M3 12h18"/><path d="M3 19h18"/></svg>
						{:else}
							<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M15 12H3"/><path d="M17 19H3"/></svg>
						{/if}
					</button>
					{#if alignDropdown}
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="fmt-dropdown align-dropdown" onclick={(e) => e.stopPropagation()}>
							<button class:active={(editorState, editor.isActive({ textAlign: 'left' }))} onclick={() => { editor?.chain().focus().setTextAlign('left').run(); alignDropdown = false; }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M15 12H3"/><path d="M17 19H3"/></svg>
								Left
							</button>
							<button class:active={(editorState, editor.isActive({ textAlign: 'center' }))} onclick={() => { editor?.chain().focus().setTextAlign('center').run(); alignDropdown = false; }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M17 12H7"/><path d="M19 19H5"/></svg>
								Center
							</button>
							<button class:active={(editorState, editor.isActive({ textAlign: 'right' }))} onclick={() => { editor?.chain().focus().setTextAlign('right').run(); alignDropdown = false; }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 5H3"/><path d="M21 12H9"/><path d="M21 19H7"/></svg>
								Right
							</button>
							<button class:active={(editorState, editor.isActive({ textAlign: 'justify' }))} onclick={() => { editor?.chain().focus().setTextAlign('justify').run(); alignDropdown = false; }}>
								<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h18"/><path d="M3 12h18"/><path d="M3 19h18"/></svg>
								Justify
							</button>
						</div>
					{/if}
				</div>

				<div class="fmt-sep"></div>

				<!-- Indent / Outdent -->
				<button class="fmt-btn" onclick={() => {
					if (!editor) return;
					// Try list indent first - run() returns true if it succeeded
					const sank = editor.chain().focus().sinkListItem('listItem').run();
					if (!sank) {
						const sankTask = editor.chain().focus().sinkListItem('taskItem').run();
						if (!sankTask && editor.state.selection.empty) {
							editor.chain().focus().insertContent('\t').run();
						}
					}
				}} title="Indent (Tab)">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="4" x2="21" y2="4"/><line x1="11" y1="9" x2="21" y2="9"/><line x1="11" y1="14" x2="21" y2="14"/><line x1="3" y1="19" x2="21" y2="19"/><polyline points="3 9 7 11.5 3 14"/></svg>
				</button>
				<button class="fmt-btn" onclick={() => {
					if (!editor) return;
					const lifted = editor.chain().focus().liftListItem('listItem').run();
					if (!lifted) {
						const liftedTask = editor.chain().focus().liftListItem('taskItem').run();
						if (!liftedTask && editor.state.selection.empty) {
							// Remove leading tab/spaces from current line
							const { from } = editor.state.selection;
							const pos = editor.state.doc.resolve(from);
							const lineStart = pos.start(pos.depth);
							const lineText = editor.state.doc.textBetween(lineStart, pos.end(pos.depth));
							if (lineText.startsWith('\t')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 1);
									return true;
								}).run();
							} else if (lineText.startsWith('    ')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 4);
									return true;
								}).run();
							} else if (lineText.startsWith('  ')) {
								editor.chain().focus().command(({ tr }) => {
									tr.delete(lineStart, lineStart + 2);
									return true;
								}).run();
							}
						}
					}
				}} title="Outdent (Shift+Tab)">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="4" x2="21" y2="4"/><line x1="11" y1="9" x2="21" y2="9"/><line x1="11" y1="14" x2="21" y2="14"/><line x1="3" y1="19" x2="21" y2="19"/><polyline points="7 9 3 11.5 7 14"/></svg>
				</button>

				{/if}
			</div>
		{/if}
	{/if}

	<!-- Hidden file inputs for Insert dropdown -->
	<input type="file" id="insert-image-input" accept="image/*" style="display:none" onchange={(e) => {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (file) insertImage(file);
		(e.target as HTMLInputElement).value = '';
	}} />
	<input type="file" id="insert-file-input" style="display:none" onchange={(e) => {
		const file = (e.target as HTMLInputElement).files?.[0];
		if (file) {
			if (file.type.startsWith('image/')) insertImage(file);
			else if (file.type === 'application/pdf') insertPdf(file);
			else insertFileAttachment(file);
		}
		(e.target as HTMLInputElement).value = '';
	}} />
</div>

{#if linkContextMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="link-context-overlay" onclick={closeLinkContextMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="link-context-menu" style="left: {linkContextMenu.x}px; top: {linkContextMenu.y}px" onclick={(e) => e.stopPropagation()}>
			<div class="link-context-url">{linkContextMenu.href}</div>
			<button onclick={linkMenuOpen}>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6" />
					<polyline points="15 3 21 3 21 9" />
					<line x1="10" y1="14" x2="21" y2="3" />
				</svg>
				Open Link
			</button>
			{#if isFileLink(linkContextMenu.href)}
			<button onclick={linkMenuSaveAs}>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>
				</svg>
				Save As...
			</button>
			{/if}
			<button onclick={linkMenuCopy}>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
					<path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
				</svg>
				Copy Link
			</button>
			<button onclick={linkMenuEdit}>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
					<path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
				</svg>
				Edit Link
			</button>
			<div class="link-context-sep"></div>
			<button class="danger" onclick={linkMenuRemove}>
				<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
					<path d="M18 6L6 18M6 6l12 12" />
				</svg>
				Remove Link
			</button>
		</div>
	</div>
{/if}

{#if textContextMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="text-ctx-overlay" onclick={closeTextContextMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="text-ctx-menu" style="left: {textContextMenu.x}px; top: {textContextMenu.y}px" onclick={(e) => e.stopPropagation()}>
			<button onclick={ctxCut}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><line x1="20" y1="4" x2="8.12" y2="15.88"/><line x1="14.47" y1="14.48" x2="20" y2="20"/><line x1="8.12" y1="8.12" x2="12" y2="12"/></svg>
				Cut
				<span class="text-ctx-shortcut">{modKey}+X</span>
			</button>
			<button onclick={ctxCopy}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
				Copy
				<span class="text-ctx-shortcut">{modKey}+C</span>
			</button>
			<button onclick={ctxPaste}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4h2a2 2 0 012 2v14a2 2 0 01-2 2H6a2 2 0 01-2-2V6a2 2 0 012-2h2"/><rect x="8" y="2" width="8" height="4" rx="1" ry="1"/></svg>
				Paste
				<span class="text-ctx-shortcut">{modKey}+V</span>
			</button>
			<div class="text-ctx-sep"></div>
			<button onclick={ctxSelectAll}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M8 7h8M8 12h8M8 17h8"/></svg>
				Select All
				<span class="text-ctx-shortcut">{modKey}+A</span>
			</button>
			<div class="text-ctx-sep"></div>
			<!-- Heading submenu -->
			<div class="text-ctx-submenu-wrap" onmouseenter={() => ctxHeadingSubmenu = true} onmouseleave={() => ctxHeadingSubmenu = false}>
				<button class="has-submenu">
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h16M4 6v12M20 6v12"/></svg>
					Heading
					<svg class="submenu-arrow" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="9 6 15 12 9 18"/></svg>
				</button>
				{#if ctxHeadingSubmenu}
					<div class="text-ctx-submenu" class:flip-left={textContextMenu?.submenuLeft}>
						<button class:active={(editorState, editor?.isActive('heading', { level: 1 }))} onclick={() => ctxSetHeading(1)}>Heading 1</button>
						<button class:active={(editorState, editor?.isActive('heading', { level: 2 }))} onclick={() => ctxSetHeading(2)}>Heading 2</button>
						<button class:active={(editorState, editor?.isActive('heading', { level: 3 }))} onclick={() => ctxSetHeading(3)}>Heading 3</button>
						<button class:active={(editorState, editor?.isActive('heading', { level: 4 }))} onclick={() => ctxSetHeading(4)}>Heading 4</button>
						<div class="text-ctx-sep"></div>
						<button class:active={(editorState, editor?.isActive('paragraph'))} onclick={ctxSetParagraph}>Paragraph</button>
					</div>
				{/if}
			</div>
			<div class="text-ctx-sep"></div>
			<button onclick={ctxBold}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1"><path d="M6 4h8a4 4 0 014 4 4 4 0 01-4 4H6zm0 8h9a4 4 0 014 4 4 4 0 01-4 4H6z"/></svg>
				Bold
				<span class="text-ctx-shortcut">{modKey}+B</span>
			</button>
			<button onclick={ctxItalic}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="19" y1="4" x2="10" y2="4"/><line x1="14" y1="20" x2="5" y2="20"/><line x1="15" y1="4" x2="9" y2="20"/></svg>
				Italic
				<span class="text-ctx-shortcut">{modKey}+I</span>
			</button>
			<button onclick={ctxUnderline}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3v7a6 6 0 006 6 6 6 0 006-6V3"/><line x1="4" y1="21" x2="20" y2="21"/></svg>
				Underline
				<span class="text-ctx-shortcut">{modKey}+U</span>
			</button>
			<button onclick={ctxStrike}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 4H9a3 3 0 00-3 3 3 3 0 003 3h6"/><line x1="4" y1="12" x2="20" y2="12"/><path d="M8 20h7a3 3 0 003-3 3 3 0 00-3-3H8"/></svg>
				Strikethrough
			</button>
			<button onclick={ctxHighlight}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 013 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
				Highlight
			</button>
			<div class="text-ctx-sep"></div>
			<button onclick={ctxLink}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/></svg>
				Add Link
				<span class="text-ctx-shortcut">{modKey}+K</span>
			</button>
			<button onclick={ctxCode}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
				Inline Code
			</button>
			<button onclick={ctxCodeBlock}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><polyline points="9 8 5 12 9 16"/><polyline points="15 8 19 12 15 16"/></svg>
				Code Block
			</button>
			<button onclick={ctxBlockquote}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor"><path d="M3 6h4v4l-2 6H3l2-6H3V6zm10 0h4v4l-2 6h-2l2-6h-2V6z"/></svg>
				Quote
			</button>
			<button onclick={ctxDetails}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><polyline points="10 8 14 12 10 16"/></svg>
				Collapsible Section
			</button>
			<button onclick={ctxCallout}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="14" rx="2"/><line x1="7" y1="5" x2="7" y2="19"/></svg>
				Callout
			</button>
			<button onclick={ctxTimestamp}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 7.5V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h6"/><line x1="3" y1="10" x2="21" y2="10"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="16" y1="2" x2="16" y2="6"/><circle cx="18" cy="17" r="4"/><path d="M18 15.5v1.5l1 1"/></svg>
				Date &amp; Time
			</button>
			<div class="text-ctx-sep"></div>
			<button onclick={ctxBulletList}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><circle cx="3.5" cy="6" r="1.5" fill="currentColor"/><circle cx="3.5" cy="12" r="1.5" fill="currentColor"/><circle cx="3.5" cy="18" r="1.5" fill="currentColor"/></svg>
				Bullet List
			</button>
			<button onclick={ctxOrderedList}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="10" y1="6" x2="21" y2="6"/><line x1="10" y1="12" x2="21" y2="12"/><line x1="10" y1="18" x2="21" y2="18"/><text x="1" y="8" font-size="8" fill="currentColor" stroke="none" font-weight="600">1</text><text x="1" y="14" font-size="8" fill="currentColor" stroke="none" font-weight="600">2</text><text x="1" y="20" font-size="8" fill="currentColor" stroke="none" font-weight="600">3</text></svg>
				Numbered List
			</button>
			<button onclick={ctxTaskList}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><polyline points="4.5 6.5 6 8 8.5 4.5"/><line x1="13" y1="6.5" x2="21" y2="6.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><line x1="13" y1="17.5" x2="21" y2="17.5"/></svg>
				Task List
			</button>
			{#if $appConfig?.ai_provider}
				<div class="text-ctx-sep"></div>
				<button onclick={openAiMenu}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
						<path d="M12 2a4 4 0 014 4v1a1 1 0 001 1h1a4 4 0 010 8h-1a1 1 0 00-1 1v1a4 4 0 01-8 0v-1a1 1 0 00-1-1H6a4 4 0 010-8h1a1 1 0 001-1V6a4 4 0 014-4z" />
						<circle cx="12" cy="12" r="2" />
					</svg>
					AI Actions
				</button>
			{/if}
		</div>
	</div>
{/if}

{#if tableContextMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="table-ctx-overlay" onclick={closeTableContextMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="table-ctx-menu" style="left: {tableContextMenu.x}px; top: {tableContextMenu.y}px; max-height: calc(100vh - {tableContextMenu.y}px - 8px); overflow-y: auto;" onclick={(e) => e.stopPropagation()}>
			<button onclick={tblAddRowBefore}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M3 12h18M3 18h18"/><path d="M12 3v3"/><polyline points="9 4.5 12 2 15 4.5"/></svg>
				Add Row Above
			</button>
			<button onclick={tblAddRowAfter}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M3 12h18M3 18h18"/><path d="M12 21v-3"/><polyline points="9 19.5 12 22 15 19.5"/></svg>
				Add Row Below
			</button>
			<button class="danger" onclick={tblDeleteRow}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12h18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
				Delete Row
			</button>
			<div class="table-ctx-sep"></div>
			<button onclick={tblAddColBefore}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3v18M12 3v18M18 3v18"/><path d="M3 12h3"/><polyline points="4.5 9 2 12 4.5 15"/></svg>
				Add Column Left
			</button>
			<button onclick={tblAddColAfter}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3v18M12 3v18M18 3v18"/><path d="M21 12h-3"/><polyline points="19.5 9 22 12 19.5 15"/></svg>
				Add Column Right
			</button>
			<button class="danger" onclick={tblDeleteCol}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v18"/><line x1="18" y1="6" x2="6" y2="18"/></svg>
				Delete Column
			</button>
			<div class="table-ctx-sep"></div>
			<button onclick={tblMergeCells}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="M14 9l-4 3 4 3"/></svg>
				Merge Cells
			</button>
			<button onclick={tblSplitCell}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18"/><path d="M8 9l4 3-4 3"/><path d="M16 9l-4 3 4 3"/></svg>
				Split Cell
			</button>
			<div class="table-ctx-sep"></div>
			<button onclick={tblToggleHeaderRow}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="3" y1="9" x2="21" y2="9"/><rect x="4" y="4" width="16" height="4" rx="1" fill="currentColor" opacity="0.2"/></svg>
				Toggle Header Row
			</button>
			<button onclick={tblToggleHeaderCol}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><rect x="4" y="4" width="4" height="16" rx="1" fill="currentColor" opacity="0.2"/></svg>
				Toggle Header Column
			</button>
			<div class="table-ctx-sep"></div>
			<div class="table-ctx-color-label">Cell Color</div>
			<div class="table-ctx-colors">
				{#each cellColors as color}
					<button
						class="table-ctx-color-swatch"
						title={color.name}
						style="background: {color.value || 'var(--bg-primary)'}; {color.value === '' ? 'border: 1px dashed var(--border-color);' : ''}"
						onclick={() => tblSetCellColor(color.value)}
					></button>
				{/each}
			</div>
			<div class="table-ctx-sep"></div>
			{#if tableContextMenu.hasStyling}
			<button onclick={resetTableToMarkdown} title="Strip cell colors and merged cells, output as plain markdown">
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 11-3-6.7"/><polyline points="21 4 21 10 15 10"/></svg>
				Reset to Markdown
			</button>
			<div class="table-ctx-sep"></div>
			{/if}
			<button class="danger" onclick={tblDeleteTable}>
				<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>
				Delete Table
			</button>
		</div>
	</div>
{/if}

{#if imageToolbar}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="img-toolbar-overlay" onclick={() => (imageToolbar = null)}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="img-toolbar" style="left: {imageToolbar.x}px; top: {imageToolbar.y}px" onclick={(e) => e.stopPropagation()}>
			<button class:active={imageToolbar.size === 'small'} onclick={() => setImageSize('small')} title="Small (33%)">S</button>
			<button class:active={imageToolbar.size === 'medium'} onclick={() => setImageSize('medium')} title="Medium (50%)">M</button>
			<button class:active={imageToolbar.size === 'full'} onclick={() => setImageSize('full')} title="Full width">L</button>
			{#if !isMobile && !imageToolbar.src.startsWith('imgproxy:') && !imageToolbar.src.startsWith('http://imgproxy.localhost')}
				<span class="img-toolbar-sep"></span>
				<button onclick={copyImageToClipboard} title="Copy image">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
				</button>
				<button onclick={openImageInApp} title="Open in default app">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/></svg>
				</button>
			{/if}
		</div>
	</div>
{/if}

{#if copyToast}
	<div class="copy-toast" class:done={copyToast === 'done'}>
		{#if copyToast === 'copying'}
			<svg class="copy-toast-spinner" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
				<circle cx="12" cy="12" r="10" opacity="0.25" />
				<path d="M12 2a10 10 0 019.95 9" />
			</svg>
			Copying...
		{:else}
			<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
				<polyline points="20 6 9 17 4 12" />
			</svg>
			Copied
		{/if}
	</div>
{/if}

{#if mathModal}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="math-modal-overlay" onclick={cancelMathModal}>
		<div class="math-modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Math editor">
			<div class="math-modal-header">
				<span>{mathModal.editPos !== null ? 'Edit' : 'Insert'} {mathModal.kind === 'block' ? 'Math Block' : 'Inline Math'}</span>
				<button type="button" class="math-modal-close" onclick={cancelMathModal} aria-label="Close">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<textarea
				class="math-modal-input"
				placeholder="LaTeX, e.g. E = mc^2"
				bind:value={mathModal.tex}
				onkeydown={(e) => {
					if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commitMathModal(); }
					if (e.key === 'Escape') { e.preventDefault(); cancelMathModal(); }
				}}
				autofocus
			></textarea>
			<div class="math-modal-preview">
				{#if mathModal.tex.trim()}
					<div>{@html renderMathPreview(mathModal.tex, mathModal.kind === 'block')}</div>
				{:else}
					<span class="math-modal-preview-empty">Preview appears here…</span>
				{/if}
			</div>
			<div class="math-modal-footer">
				<span class="math-modal-hint">{modKey}+Enter to {mathModal.editPos !== null ? 'update' : 'insert'} · Esc to cancel</span>
				<div class="math-modal-actions">
					<button type="button" onclick={cancelMathModal}>Cancel</button>
					<button type="button" class="primary" onclick={commitMathModal} disabled={!mathModal.tex.trim()}>
						{mathModal.editPos !== null ? 'Update' : 'Insert'}
					</button>
				</div>
			</div>
		</div>
	</div>
{/if}

{#if secretModal}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="math-modal-overlay" onclick={cancelSecretModal}>
		<div class="math-modal secret-modal" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Encrypted secret editor">
			<div class="math-modal-header">
				<span>Insert Secret</span>
				<button type="button" class="math-modal-close" onclick={cancelSecretModal} aria-label="Close">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<input
				class="secret-modal-title"
				type="text"
				placeholder="Title"
				bind:value={secretModal.title}
				bind:this={secretTitleInput}
				onkeydown={(e) => {
					if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commitSecretModal(); }
					if (e.key === 'Escape') { e.preventDefault(); cancelSecretModal(); }
				}}
			/>
			<textarea
				class="math-modal-input secret-modal-text"
				placeholder="Secret text"
				bind:value={secretModal.text}
				onkeydown={(e) => {
					if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commitSecretModal(); }
					if (e.key === 'Escape') { e.preventDefault(); cancelSecretModal(); }
				}}
			></textarea>
			<div class="secret-modal-fields">
				<input
					type="password"
					placeholder="Passphrase"
					autocomplete="new-password"
					bind:value={secretModal.passphrase}
					onkeydown={(e) => {
						if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commitSecretModal(); }
						if (e.key === 'Escape') { e.preventDefault(); cancelSecretModal(); }
					}}
				/>
				<input
					type="password"
					placeholder="Confirm passphrase"
					autocomplete="new-password"
					bind:value={secretModal.confirmPassphrase}
					onkeydown={(e) => {
						if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); commitSecretModal(); }
						if (e.key === 'Escape') { e.preventDefault(); cancelSecretModal(); }
					}}
				/>
			</div>
			{#if secretModal.error}
				<div class="secret-modal-error">{secretModal.error}</div>
			{/if}
			<div class="math-modal-footer">
				<span class="math-modal-hint">Stored as a portable helix-secret markdown block</span>
				<div class="math-modal-actions">
					<button type="button" onclick={cancelSecretModal} disabled={secretModal.busy}>Cancel</button>
					<button type="button" class="primary" onclick={commitSecretModal} disabled={secretModal.busy || !secretModal.text || !secretModal.passphrase || !secretModal.confirmPassphrase}>
						{secretModal.busy ? 'Encrypting...' : 'Encrypt'}
					</button>
				</div>
			</div>
		</div>
	</div>
{/if}

{#if viewerImportPickerOpen}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="viewer-import-overlay" onclick={() => (viewerImportPickerOpen = false)}>
		<div class="viewer-import-picker" onclick={(e) => e.stopPropagation()} role="dialog">
			<div class="viewer-import-header">
				<span>Import to folder</span>
				<button type="button" class="viewer-import-close" onclick={() => (viewerImportPickerOpen = false)} aria-label="Close">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
				</button>
			</div>
			<div class="viewer-import-list">
				<button type="button" class="viewer-import-item" onclick={() => viewerImportTo('')} disabled={viewerImportBusy}>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
					Vault root
				</button>
				{#each viewerFlatNotebooks as nb (nb.path)}
					<button type="button" class="viewer-import-item" style="padding-left: {12 + nb.depth * 16}px" onclick={() => viewerImportTo(nb.path)} disabled={viewerImportBusy}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
						{nb.name}
					</button>
				{/each}
			</div>
		</div>
	</div>
{/if}

{#if tagMenu && $activeNote}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div class="tag-menu-overlay" onclick={() => (tagMenu = null)}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<div class="tag-menu" style="left: {tagMenu.x}px; top: {tagMenu.y}px" onclick={(e) => e.stopPropagation()}>
			{#if $activeNote.meta.tags.length > 0}
				<div class="tag-menu-list">
					{#each $activeNote.meta.tags as tag}
						<span class="tag-menu-chip">
							#{tag}
							<button class="tag-menu-remove" onclick={() => removeActiveNoteTag(tag)} title="Remove tag" aria-label="Remove tag">
								<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
							</button>
						</span>
					{/each}
				</div>
			{/if}
			<TagSuggestInput
				existing={$activeNote.meta.tags}
				placeholder="Add tag..."
				onsubmit={(t) => addActiveNoteTag(t)}
				oncancel={() => (tagMenu = null)}
			/>
		</div>
	</div>
{/if}

{#if codeLangDropdown}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="code-lang-overlay" onclick={closeCodeLangDropdown}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="code-lang-dropdown" style="left: {codeLangDropdown.x}px; top: {codeLangDropdown.y}px" onclick={(e) => e.stopPropagation()}>
			<input class="code-lang-search" type="text" placeholder="Search..." bind:this={codeLangInput} bind:value={codeLangSearch} onkeydown={(e) => {
				if (e.key === 'Enter' && codeLangFiltered.length > 0) { e.preventDefault(); e.stopPropagation(); selectCodeLang(codeLangFiltered[0]); }
				if (e.key === 'Escape') closeCodeLangDropdown();
			}} />
			{#if !codeLangSearch}
				<button
					class="code-lang-option"
					class:active={codeLangDropdown.current === ''}
					onclick={() => selectCodeLang('')}
				>auto</button>
			{/if}
			{#each codeLangFiltered as lang}
				<button
					class="code-lang-option"
					class:active={codeLangDropdown.current === lang}
					onclick={() => selectCodeLang(lang)}
				>{lang}</button>
			{/each}
			{#if codeLangFiltered.length === 0}
				<div class="code-lang-empty">No match</div>
			{/if}
		</div>
	</div>
{/if}

{#if slashMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="slash-menu-overlay" onclick={closeSlashMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="slash-menu" style="left: {slashMenu.x}px; top: {slashMenu.y}px" onclick={(e) => e.stopPropagation()}>
			{#if slashTablePicker}
				<div class="slash-table-picker">
					<div class="slash-table-picker-grid">
						{#each Array(8) as _, r}
							{#each Array(10) as _, c}
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="table-picker-cell"
									class:active={r < slashTableHover.rows && c < slashTableHover.cols}
									onmouseenter={() => slashTableHover = { rows: r + 1, cols: c + 1 }}
									onmousedown={(e) => { e.preventDefault(); slashInsertTable(r + 1, c + 1); }}
								></div>
							{/each}
						{/each}
					</div>
					<div class="slash-table-picker-label">
						{slashTableHover.rows > 0 ? `${slashTableHover.rows} x ${slashTableHover.cols}` : 'Select table size'}
					</div>
				</div>
			{:else if slashColorPicker}
				<div class="slash-color-picker">
					<div class="slash-color-swatches">
						{#each colorPresets as c}
							<button class="slash-color-swatch" style="background: {c}" title={c} aria-label={c} onmousedown={(e) => { e.preventDefault(); insertColor(c); }}></button>
						{/each}
					</div>
					<div class="slash-color-row">
						<input type="color" class="slash-color-native" value={/^#[0-9a-fA-F]{6}$/.test(slashColorHex) ? slashColorHex : '#4b6abf'} oninput={(e) => { slashColorHex = (e.target as HTMLInputElement).value; }} title="Pick a color" />
						<input
							type="text"
							class="slash-color-input"
							bind:this={slashColorInputEl}
							value={slashColorHex}
							placeholder="#hex or rgb(...)"
							oninput={(e) => { slashColorHex = (e.target as HTMLInputElement).value; }}
							onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); insertColor(slashColorHex); } else if (e.key === 'Escape') { e.preventDefault(); closeSlashMenu(); } }}
						/>
						<button class="slash-color-insert" onmousedown={(e) => { e.preventDefault(); insertColor(slashColorHex); }}>Insert</button>
					</div>
				</div>
			{:else if slashFiltered.length === 0}
				<div class="slash-menu-empty">No matching commands</div>
			{:else}
				{#each slashFiltered as cmd, i}
					<button
						class="slash-menu-item"
						class:selected={i === slashSelectedIndex}
						onmouseenter={() => slashSelectedIndex = i}
						onmousedown={(e) => { e.preventDefault(); executeSlashCommand(i); }}
					>
						<span class="slash-menu-icon">{@html cmd.icon}</span>
						<span class="slash-menu-label">{cmd.label}</span>
					</button>
				{/each}
			{/if}
		</div>
	</div>
{/if}

{#if taskMetaMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="slash-menu-overlay" onclick={closeTaskMetaMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="slash-menu" style="left: {taskMetaMenu.x}px; top: {taskMetaMenu.y}px" onclick={(e) => e.stopPropagation()}>
			{#if taskMetaFiltered.length === 0}
				<div class="slash-menu-empty">No match</div>
			{:else}
				{#each taskMetaFiltered as item, i}
					<button
						class="slash-menu-item"
						class:selected={i === taskMetaSelectedIndex}
						onmouseenter={() => taskMetaSelectedIndex = i}
						onmousedown={(e) => { e.preventDefault(); selectTaskMeta(i); }}
					>
						<span class="slash-menu-icon">{@html item.icon}</span>
						<span class="slash-menu-label">{item.label}</span>
					</button>
				{/each}
			{/if}
		</div>
	</div>
{/if}

{#if taskDuePicker}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="slash-menu-overlay" onclick={() => { taskDuePicker = null; editor?.commands.focus(); }}>
		<input
			type="date"
			class="task-due-input"
			bind:this={taskDueInputEl}
			style="left: {taskDuePicker.x}px; top: {taskDuePicker.y}px"
			onclick={(e) => e.stopPropagation()}
			onchange={(e) => applyTaskDue((e.currentTarget as HTMLInputElement).value)}
			onkeydown={(e) => { if (e.key === 'Escape') { taskDuePicker = null; editor?.commands.focus(); } }}
		/>
	</div>
{/if}

{#if wikiLinkMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="wiki-link-overlay" onclick={closeWikiLinkMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="wiki-link-menu" style="left: {wikiLinkMenu.x}px; top: {wikiLinkMenu.y}px" onclick={(e) => e.stopPropagation()}>
			{#if wikiLinkFiltered.length === 0}
				<div class="wiki-link-empty">
					{wikiLinkMenu.query ? 'No matching notes' : 'Type to search notes...'}
				</div>
			{:else}
				{#each wikiLinkFiltered.slice(0, 12) as entry, i}
					<button
						class="wiki-link-item"
						class:selected={i === wikiLinkSelectedIndex}
						onmouseenter={() => wikiLinkSelectedIndex = i}
						onmousedown={(e) => { e.preventDefault(); if (wikiLinkDisambigEntries) { insertWikiLink({ ...entry, title: wikiLinkDisambigDisplay || entry.title }, wikiLinkDisambigRef || undefined); } else { insertWikiLink(entry); } }}
					>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
						<span class="wiki-link-title-col">
							<span class="wiki-link-title">{entry.title}</span>
							{#if wikiLinkFolderPath(entry)}
								<span class="wiki-link-folder">{wikiLinkFolderPath(entry)}</span>
							{/if}
						</span>
					</button>
				{/each}
			{/if}
		</div>
	</div>
{/if}

{#if wikiLinkNavDisambig}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="wiki-link-overlay" onclick={() => wikiLinkNavDisambig = null}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="wiki-link-menu" style="left: {wikiLinkNavDisambig.x}px; top: {wikiLinkNavDisambig.y}px" onclick={(e) => e.stopPropagation()}>
			<div class="wiki-link-disambig-header">Multiple notes found - choose one:</div>
			{#each wikiLinkNavDisambig.entries as entry, i}
				<button
					class="wiki-link-item"
					class:selected={i === wikiLinkNavDisambigIndex}
					onmouseenter={() => wikiLinkNavDisambigIndex = i}
					onmousedown={(e) => { e.preventDefault(); navigateToWikiLinkDirect(entry); }}
				>
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
					<span class="wiki-link-title-col">
						<span class="wiki-link-title">{entry.title}</span>
						<span class="wiki-link-folder">{wikiLinkFolderPath(entry) || '(vault root)'}</span>
					</span>
				</button>
			{/each}
		</div>
	</div>
{/if}

{#if aiMenu}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="ai-menu-overlay" class:mobile={isMobile} onclick={closeAiMenu}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="ai-menu" class:mobile={isMobile} style={isMobile ? '' : `left: ${aiMenu.x}px; top: ${aiMenu.y}px`} onclick={(e) => e.stopPropagation()}>
			{#if aiResult !== null || aiLoading}
				<!-- Result view -->
				<div class="ai-result-header">
					<span class="ai-result-title">
						{#if aiLoading}
							<svg class="ai-spinner" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" opacity="0.25" /><path d="M12 2a10 10 0 019.95 9" /></svg>
							Generating...
						{:else}
							AI Result
						{/if}
					</span>
					<button class="ai-result-close" onclick={closeAiMenu}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
					</button>
				</div>
				{#if aiError}
					<div class="ai-error">{aiError}</div>
				{:else}
					<div class="ai-result-body">{aiResult}</div>
				{/if}
				{#if !aiLoading && aiResult && !aiError}
					<div class="ai-result-actions">
						<button class="ai-action-btn apply" onclick={aiApplyResult}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
							{aiEmptyNote ? 'Insert Note' : aiWholeNote ? 'Apply to Note' : 'Replace'}
						</button>
						<button class="ai-action-btn discard" onclick={aiDiscard}>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" /></svg>
							Discard
						</button>
					</div>
				{/if}
			{:else if aiShowCustom}
				<!-- Custom prompt input -->
				<div class="ai-custom-header">
					<button class="ai-back-btn" onclick={() => aiShowCustom = false}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
					</button>
					<span>Custom Prompt</span>
				</div>
				<div class="ai-custom-body">
					<textarea
						class="ai-custom-input"
						placeholder="Tell AI what to do with the selected text..."
						bind:value={aiCustomPrompt}
						onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); runAiAction('custom', aiCustomPrompt); } }}
						use:autofocus
					></textarea>
					<button class="ai-custom-submit" onclick={() => runAiAction('custom', aiCustomPrompt)} disabled={!aiCustomPrompt.trim()}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" /></svg>
						Send
					</button>
				</div>
			{:else if aiTranslateMenu}
				<!-- Translate submenu -->
				<div class="ai-custom-header">
					<button class="ai-back-btn" onclick={() => aiTranslateMenu = false}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
					</button>
					<span>Translate to</span>
				</div>
				<button class="ai-menu-item" onclick={() => runAiAction('translate_en')}>English</button>
				<button class="ai-menu-item" onclick={() => runAiAction('translate_nl')}>Dutch</button>
				<button class="ai-menu-item" onclick={() => runAiAction('translate_de')}>German</button>
				<button class="ai-menu-item" onclick={() => runAiAction('translate_fr')}>French</button>
				<button class="ai-menu-item" onclick={() => runAiAction('translate_es')}>Spanish</button>
			{:else if aiEmptyNote}
				<!-- Empty note - generate from prompt -->
				<div class="ai-menu-label">Generate Note</div>
				<div class="ai-custom-body">
					<textarea
						class="ai-custom-input"
						placeholder="Describe the note you want to create..."
						bind:value={aiCustomPrompt}
						onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); runAiAction('custom', aiCustomPrompt); } }}
						use:autofocus
					></textarea>
					<button class="ai-custom-submit" onclick={() => runAiAction('custom', aiCustomPrompt)} disabled={!aiCustomPrompt.trim()}>
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 8V4l-2-2"/><rect x="4" y="8" width="16" height="12" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><path d="M9 13v2"/><path d="M15 13v2"/></svg>
						Generate
					</button>
				</div>
			{:else}
				<!-- Action list -->
				<div class="ai-menu-label">{aiWholeNote ? 'AI Actions (Entire Note)' : 'AI Actions'}</div>
				<button class="ai-menu-item" onclick={() => runAiAction('improve')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 013 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
					Improve Writing
				</button>
				<button class="ai-menu-item" onclick={() => runAiAction('fix_grammar')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
					Fix Grammar
				</button>
				<button class="ai-menu-item" onclick={() => runAiAction('shorter')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h16"/><path d="M4 6h10"/></svg>
					Make Shorter
				</button>
				<button class="ai-menu-item" onclick={() => runAiAction('longer')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6h16"/><path d="M4 12h16"/><path d="M4 18h10"/></svg>
					Make Longer
				</button>
				<div class="ai-menu-sep"></div>
				<button class="ai-menu-item" onclick={() => runAiAction('professional')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="20" height="14" rx="2" ry="2"/><path d="M16 7V5a2 2 0 00-2-2h-4a2 2 0 00-2 2v2"/></svg>
					Professional Tone
				</button>
				<button class="ai-menu-item" onclick={() => runAiAction('friendly')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
					Friendly Tone
				</button>
				<div class="ai-menu-sep"></div>
				<button class="ai-menu-item" onclick={() => runAiAction('summarize')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="4" y1="6" x2="20" y2="6"/><line x1="4" y1="10" x2="16" y2="10"/><line x1="4" y1="14" x2="12" y2="14"/></svg>
					Summarize
				</button>
				<button class="ai-menu-item" onclick={() => runAiAction('explain')}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>
					Explain
				</button>
				<button class="ai-menu-item" onclick={() => aiTranslateMenu = true}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 8l6 6"/><path d="M4 14l6-6 2-3"/><path d="M2 5h12"/><path d="M7 2h1"/><path d="M22 22l-5-10-5 10"/><path d="M14 18h6"/></svg>
					Translate
					<span class="ai-menu-arrow">
						<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
					</span>
				</button>
				<div class="ai-menu-sep"></div>
				<button class="ai-menu-item" onclick={() => aiShowCustom = true}>
					<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13" /><polygon points="22 2 15 22 11 13 2 9 22 2" /></svg>
					Custom Prompt...
				</button>
			{/if}
		</div>
	</div>
{/if}

{#if linkModal}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="link-modal-overlay" onclick={linkModalCancel}>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="link-modal" onclick={(e) => e.stopPropagation()}>
			<div class="link-modal-header">
				<svg width="28" height="28" viewBox="0 0 48 48" fill="none">
					<rect width="48" height="48" rx="12" fill="var(--accent)" />
					<circle cx="16" cy="16" r="3.5" fill="white" opacity="0.9" />
					<circle cx="32" cy="16" r="3.5" fill="white" opacity="0.9" />
					<circle cx="16" cy="32" r="3.5" fill="white" opacity="0.9" />
					<circle cx="32" cy="32" r="3.5" fill="white" opacity="0.9" />
					<line x1="19" y1="18" x2="29" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
					<line x1="29" y1="18" x2="19" y2="30" stroke="white" stroke-width="2" stroke-linecap="round" opacity="0.7" />
				</svg>
				<span>Insert Link</span>
			</div>
			<input
				type="text"
				class="link-modal-input"
				bind:this={linkModalInput}
				bind:value={linkModalUrl}
				oninput={() => { linkSuggestIndex = 0; }}
				onkeydown={(e) => {
					if (linkSuggestFiltered.length > 0) {
						if (e.key === 'ArrowDown') { e.preventDefault(); linkSuggestIndex = Math.min(linkSuggestIndex + 1, linkSuggestFiltered.length - 1); return; }
						if (e.key === 'ArrowUp') { e.preventDefault(); linkSuggestIndex = Math.max(linkSuggestIndex - 1, 0); return; }
						if (e.key === 'Enter') { e.preventDefault(); linkModalSelectNote(linkSuggestFiltered[linkSuggestIndex]); return; }
					} else {
						if (e.key === 'Enter') { e.preventDefault(); linkModalConfirm(); }
					}
					if (e.key === 'Escape') { e.preventDefault(); linkModalCancel(); }
				}}
				placeholder="URL or note name"
			/>
			{#if linkSuggestFiltered.length > 0}
				<div class="link-suggest-list">
					{#each linkSuggestFiltered as entry, i}
						<button
							class="link-suggest-item"
							class:selected={i === linkSuggestIndex}
							onmouseenter={() => linkSuggestIndex = i}
							onmousedown={(e) => { e.preventDefault(); linkModalSelectNote(entry); }}
						>
							<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V9z"/><polyline points="13 2 13 9 20 9"/></svg>
							<span class="link-suggest-title">{entry.title}</span>
						</button>
					{/each}
				</div>
			{/if}
			<div class="link-modal-actions">
				<button class="link-modal-btn cancel" onclick={linkModalCancel}>Cancel</button>
				<button class="link-modal-btn confirm" onclick={linkModalConfirm}>
					{linkModalUrl ? 'Apply' : 'Remove Link'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if showGraph}
	<GraphView onclose={() => showGraph = false} onnavigate={(path, title) => { showGraph = false; navigateToWikiLink(path, title); }} />
{/if}

<style>
	.editor-container {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg-editor);
	}

	.empty-editor {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--text-tertiary);
		gap: 12px;
	}

	.empty-icon {
		opacity: 0.5;
	}

	.empty-editor p {
		font-size: 14px;
	}

	.shortcuts-hint {
		display: flex;
		gap: 16px;
		font-size: 12px;
		margin-top: 8px;
	}

	.shortcuts-hint kbd {
		background: var(--bg-tertiary);
		border: 1px solid var(--border-color);
		border-radius: 3px;
		padding: 1px 5px;
		font-size: 11px;
		font-family: inherit;
	}

	.editor-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 20px 4px;
		flex-shrink: 0;
	}

	.nav-history-btns {
		display: flex;
		align-items: center;
		gap: 2px;
		margin-right: auto;
		flex-shrink: 0;
	}

	.nav-history-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		border: none;
		background: none;
		color: var(--text-tertiary);
		border-radius: 5px;
		cursor: pointer;
		padding: 0;
		transition: color 0.15s, background 0.15s;
	}

	.nav-history-btn:hover {
		color: var(--text-primary);
		background: var(--bg-hover);
	}

	.nav-history-btn:disabled {
		opacity: 0.3;
		cursor: default;
		pointer-events: none;
	}

	.editor-title {
		flex: 1;
	}

	.editor-title input {
		width: 100%;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 22px;
		font-weight: 700;
		outline: none;
		padding: 0;
		-webkit-user-select: text !important;
		user-select: text !important;
	}

	.editor-title input::placeholder {
		color: var(--text-tertiary);
	}

	.note-meta-bar {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 5px;
		padding: 0 20px 10px;
		flex-shrink: 0;
	}

	.note-folder {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-tertiary);
		letter-spacing: 0.01em;
	}

	.note-folder.unfiled {
		opacity: 0.6;
		font-style: italic;
	}

	.path-sep {
		font-size: 10px;
		opacity: 0.5;
	}

	.meta-divider {
		font-size: 11px;
		color: var(--text-tertiary);
		opacity: 0.4;
		user-select: none;
	}

	.note-tags-trigger {
		display: inline-flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px;
		border: none;
		background: none;
		padding: 0;
		margin: 0;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.note-tag {
		font-size: 11px;
		color: var(--text-tertiary);
		background: var(--bg-tertiary);
		padding: 1px 7px;
		border-radius: 10px;
		letter-spacing: 0.01em;
	}

	.note-tags-add {
		font-size: 11px;
		color: var(--text-tertiary);
		opacity: 0.65;
	}

	.note-tags-trigger:hover .note-tags-add {
		opacity: 1;
		color: var(--accent);
	}

	.toolbar-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.save-indicator {
		font-size: 11px;
		color: var(--text-tertiary);
		background: var(--bg-tertiary);
		padding: 2px 8px;
		border-radius: 4px;
	}

	.readonly-indicator {
		font-size: 11px;
		color: var(--accent);
		background: var(--accent-light);
		padding: 2px 8px;
		border-radius: 4px;
		font-weight: 500;
	}

	.icon-btn {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 4px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.icon-btn.active {
		color: var(--text-accent);
		background: var(--accent-light);
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		border: 0;
	}

	.editor-formatting-bar {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 6px 20px;
		border-top: 1px solid var(--border-light);
		flex-shrink: 0;
		flex-wrap: wrap;
	}

	.fmt-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 5px 7px;
		border-radius: 4px;
		font-size: 13px;
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 30px;
		height: 30px;
		position: relative;
	}

	.fmt-btn :global(svg) {
		width: 18px;
		height: 18px;
	}

	.fmt-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.fmt-btn.active {
		background: var(--accent-light);
		color: var(--text-accent);
	}

	.fmt-sep {
		width: 1px;
		height: 16px;
		background: var(--border-color);
		margin: 0 3px;
		flex-shrink: 0;
	}

	.fmt-dropdown-wrap {
		position: relative;
	}

	.fmt-dropdown {
		position: absolute;
		bottom: calc(100% + 4px);
		left: 0;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		z-index: 100;
		min-width: 140px;
	}

	.fmt-dropdown button {
		display: block;
		width: 100%;
		padding: 6px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 4px;
		text-align: left;
	}

	.fmt-dropdown button:hover {
		background: var(--bg-hover);
	}

	.fmt-dropdown button.active {
		color: var(--text-accent);
		background: var(--accent-light);
	}

	.color-indicator {
		position: absolute;
		bottom: 2px;
		left: 50%;
		transform: translateX(-50%);
		width: 12px;
		height: 2px;
		border-radius: 1px;
	}

	.color-grid-dropdown {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 4px;
		padding: 8px;
		min-width: auto;
		width: 140px;
	}

	.color-grid-dropdown .color-swatch {
		width: 28px;
		height: 28px;
		border-radius: 6px;
		border: none;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		min-width: 0;
	}

	.color-grid-dropdown .color-swatch:hover {
		transform: scale(1.15);
	}

	.insert-dropdown {
		min-width: 180px;
	}

	.insert-dropdown button {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.insert-btn {
		color: var(--accent);
	}

	.align-dropdown {
		min-width: 120px;
	}

	.align-dropdown button {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.editor-body-wrapper {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.note-search-bar {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}
	.note-search-input {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--text-primary);
		font-size: 13px;
		outline: none;
		min-width: 0;
	}
	.note-search-input::placeholder {
		color: var(--text-secondary);
	}
	.note-search-count {
		font-size: 12px;
		color: var(--text-secondary);
		white-space: nowrap;
	}
	.note-search-btn {
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}
	.note-search-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	:global(.note-search-match) {
		background: rgba(255, 200, 0, 0.3);
		border-radius: 2px;
	}
	:global(.note-search-active) {
		background: rgba(255, 150, 0, 0.6);
	}

	.editor-body-row {
		flex: 1;
		display: flex;
		flex-direction: row;
		overflow: hidden;
		min-height: 0;
	}

	.editor-body {
		flex: 1;
		overflow-y: auto;
		overflow-anchor: none;
		padding: 8px 20px;
		min-width: 0;
		position: relative;
	}

	/* Inset the scrollbar off the window's right edge so the window resize handle
	   (ResizeHandles in +layout) has clean space and doesn't swallow the scrollbar. */
	.editor-container:not(.mobile) .editor-body {
		margin-right: 8px;
	}

	.editor-body::-webkit-scrollbar {
		width: 8px;
	}

	.editor-body::-webkit-scrollbar-thumb {
		background: var(--text-tertiary);
		border-radius: 4px;
	}

	.editor-body::-webkit-scrollbar-thumb:hover {
		background: var(--text-secondary);
	}

	.editor-body:has(.source-editor) {
		overflow: hidden;
	}

	.history-panel {
		width: 240px;
		border-left: 1px solid var(--border-light);
		background: var(--bg-secondary);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		flex-shrink: 0;
	}

	.history-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 14px;
		border-bottom: 1px solid var(--border-light);
	}

	.history-header h3 {
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-tertiary);
		margin: 0;
	}

	.history-header-actions {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.history-create-btn,
	.history-close {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.history-create-btn:hover,
	.history-close:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.history-empty {
		padding: 16px 14px;
		font-size: 12px;
		color: var(--text-tertiary);
		line-height: 1.5;
	}

	.history-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 6px;
	}

	.history-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		padding: 8px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		cursor: pointer;
		text-align: left;
		transition: background 0.1s;
	}

	.history-item:hover {
		background: var(--bg-hover);
	}

	.history-item.active {
		background: var(--accent-light);
	}

	.history-date {
		font-size: 12px;
		color: var(--text-primary);
	}

	.history-item.active .history-date {
		color: var(--accent);
		font-weight: 500;
	}

	.history-size {
		font-size: 11px;
		color: var(--text-tertiary);
	}

	.history-actions {
		padding: 8px 10px;
		border-top: 1px solid var(--border-light);
	}

	.history-restore-btn {
		width: 100%;
		padding: 7px 12px;
		border: 1px solid var(--accent);
		border-radius: 6px;
		background: var(--accent-light);
		color: var(--accent);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.history-restore-btn:hover {
		background: var(--accent);
		color: white;
	}

	.outline-panel {
		width: 220px;
		border-left: 1px solid var(--border-light);
		background: var(--bg-secondary);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		flex-shrink: 0;
	}

	.outline-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 14px;
		border-bottom: 1px solid var(--border-light);
	}

	.outline-header h3 {
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-tertiary);
		margin: 0;
	}

	.outline-close {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.outline-close:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.outline-empty {
		padding: 16px 14px;
		font-size: var(--editor-font-size, 14px);
		color: var(--text-tertiary);
		line-height: 1.5;
	}

	.outline-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.outline-item {
		display: block;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		padding: 4px 14px;
		font-size: var(--editor-font-size, 14px);
		color: var(--text-secondary);
		cursor: pointer;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.5;
	}

	.outline-item:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.outline-level-1 { padding-left: 14px; font-weight: 600; }
	.outline-level-2 { padding-left: 26px; font-weight: 500; }
	.outline-level-3 { padding-left: 38px; }
	.outline-level-4 { padding-left: 50px; }
	.outline-level-5 { padding-left: 62px; }
	.outline-level-6 { padding-left: 74px; }

	.source-editor {
		width: 100%;
		height: 100%;
		border: none;
		background: none;
		color: var(--text-primary);
		font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
		font-size: var(--editor-font-size, 14px);
		line-height: 1.3;
		resize: none;
		outline: none;
		padding: 0 0 var(--editor-scroll-past-end, 65vh);
		margin: 0 auto;
		max-width: var(--editor-content-width, none);
		user-select: text;
		/* Wrap long lines instead of horizontal-scrolling (matches mobile). (issue #100) */
		white-space: pre-wrap;
		word-break: break-word;
		overflow-x: hidden;
	}

	.source-editor.with-line-numbers {
		padding-left: 48px;
		/* The line-number gutter has one fixed row per line, so wrapping would desync it.
		   Keep no-wrap (horizontal scroll) whenever line numbers are on. (issue #100) */
		white-space: pre;
		overflow-x: auto;
		/* The line-number gutter is pinned to the left edge, so don't center/cap here. (#137) */
		max-width: none;
		margin: 0;
	}

	.line-numbers-clip {
		position: absolute;
		left: 0;
		top: 0;
		bottom: 0;
		width: 44px;
		overflow: hidden;
		pointer-events: none;
	}

	.line-numbers {
		padding-top: 8px;
		font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
		font-size: var(--editor-font-size, 14px);
		line-height: 1.3;
		color: var(--text-secondary);
		opacity: 0.5;
		text-align: right;
		user-select: none;
		will-change: transform;
	}

	.line-numbers span {
		display: block;
		padding-right: 12px;
	}

	.tiptap-wrapper {
		height: 100%;
		user-select: text;
		/* Optional reading-width cap (Settings > Styling > Note Width). Default `none` = full
		   width; when set, the text column is capped and centered. Scrollbar stays at the
		   panel edge because only the inner content is constrained, not .editor-body. (#137) */
		max-width: var(--editor-content-width, none);
		margin-left: auto;
		margin-right: auto;
	}

	:global(.tiptap-wrapper .tiptap) {
		outline: none;
		min-height: 100%;
		user-select: text;
		font-size: var(--editor-font-size, 14px);
		font-family: var(--editor-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif);
		overflow: hidden;
	}

	.editor-container:not(.mobile) :global(.tiptap-wrapper .tiptap) {
		min-height: calc(100% + var(--editor-scroll-past-end, 65vh));
		padding-bottom: var(--editor-scroll-past-end, 65vh);
	}

	:global(.tiptap-wrapper .tiptap p) {
		margin: 0 0 0.75em;
		line-height: var(--editor-line-height, 1.65);
	}

	:global(.tiptap-wrapper .tiptap h1) {
		font-size: 1.75em;
		font-weight: 700;
		margin: 1.5em 0 0.5em;
		line-height: 1.2;
	}

	:global(.tiptap-wrapper .tiptap h2) {
		font-size: 1.4em;
		font-weight: 600;
		margin: 1.25em 0 0.5em;
		line-height: 1.3;
	}

	:global(.tiptap-wrapper .tiptap h3) {
		font-size: 1.2em;
		font-weight: 600;
		margin: 1em 0 0.5em;
		line-height: 1.3;
	}

	:global(.tiptap-wrapper .tiptap h4) {
		font-size: 1.05em;
		font-weight: 600;
		margin: 0.9em 0 0.4em;
		line-height: 1.35;
	}

	:global(.tiptap-wrapper .tiptap h1:first-child),
	:global(.tiptap-wrapper .tiptap h2:first-child),
	:global(.tiptap-wrapper .tiptap h3:first-child),
	:global(.tiptap-wrapper .tiptap h4:first-child) {
		margin-top: 0;
	}

	:global(.tiptap-wrapper .tiptap strong) {
		font-weight: 600;
	}

	:global(.tiptap-wrapper .tiptap code) {
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-tertiary));
		padding: 2px 6px;
		border-radius: 4px;
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		font-size: 0.9em;
	}

	:global(.tiptap-wrapper .tiptap pre) {
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-tertiary));
		border-radius: 8px;
		padding: 16px;
		margin: 1em 0;
		position: relative;
	}

	:global(.tiptap-wrapper .tiptap pre code) {
		display: block;
		overflow-x: auto;
		background: none;
		padding: 0;
		font-size: 13px;
		line-height: 1.5;
	}

	:global(.tiptap-wrapper .tiptap .code-copy-btn) {
		position: absolute;
		top: 5px;
		right: 5px;
		padding: 2px 6px;
		border-radius: 4px;
		background: transparent;
		border: 1px solid transparent;
		color: var(--text-tertiary);
		cursor: pointer;
		opacity: 0.4;
		z-index: 2;
		display: flex;
		align-items: center;
		justify-content: center;
		line-height: 0;
		transition: opacity 0.15s, background 0.15s, border-color 0.15s, color 0.15s;
	}

	:global(.tiptap-wrapper .tiptap .code-copy-btn svg) {
		width: 1em;
		height: 1em;
	}

	:global(.tiptap-wrapper .tiptap pre:hover .code-copy-btn) {
		opacity: 1;
		background: var(--bg-secondary);
		border-color: var(--border-color);
	}

	:global(.tiptap-wrapper .tiptap .code-copy-btn.copied) {
		opacity: 1;
		color: var(--text-accent);
		background: var(--accent-light);
		border-color: var(--text-accent);
	}

	:global(.tiptap-wrapper .tiptap pre)::after {
		content: attr(data-language);
		position: absolute;
		top: 5px;
		right: 34px;
		padding: 2px 6px;
		border-radius: 4px;
		background: transparent;
		color: var(--text-tertiary);
		font-size: 11px;
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		cursor: pointer;
		opacity: 0.4;
		pointer-events: none;
		z-index: 1;
		transition: opacity 0.15s;
	}

	:global(.tiptap-wrapper .tiptap pre[data-language=""])::after {
		content: '•••';
	}

	:global(.tiptap-wrapper .tiptap pre:hover)::after {
		opacity: 1;
	}

	:global(.tiptap-wrapper .tiptap .secret-block) {
		border: 1px solid var(--border);
		border-radius: 8px;
		background: color-mix(in srgb, var(--accent) 7%, var(--bg-secondary));
		margin: 1em 0;
		padding: 12px;
		user-select: text;
		box-sizing: border-box;
		max-width: 100%;
	}

	:global(.secret-block-header) {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 10px;
	}

	:global(.secret-block-title) {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		font-weight: 600;
		font-size: 13px;
		color: var(--text-primary);
	}

	:global(.secret-block-lock) {
		flex: 0 0 auto;
	}

	:global(.secret-block-badge) {
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		font-size: 11px;
		color: var(--text-tertiary);
	}

	:global(.secret-block-form),
	:global(.secret-block-actions) {
		display: flex;
		gap: 8px;
		min-width: 0;
	}

	:global(.secret-block-form input) {
		flex: 1;
		min-width: 0;
		width: 0;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-primary);
		color: var(--text-primary);
		padding: 7px 9px;
		font: inherit;
	}

	:global(.secret-block-form input:focus) {
		outline: none;
		border-color: var(--accent);
	}

	:global(.secret-block-form button),
	:global(.secret-block-actions button) {
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-primary);
		color: var(--text-primary);
		padding: 7px 10px;
		font: inherit;
		cursor: pointer;
	}

	:global(.secret-block-form button:hover:not(:disabled)),
	:global(.secret-block-actions button:hover) {
		border-color: var(--accent);
	}

	:global(.secret-block-form button:disabled) {
		opacity: 0.6;
		cursor: not-allowed;
	}

	:global(.secret-block-error) {
		color: #d32f2f;
		font-size: 12px;
		margin-top: 8px;
	}

	:global(.secret-block-plaintext) {
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
		font-family: 'JetBrains Mono', ui-monospace, monospace;
		font-size: 13px;
		line-height: 1.5;
		color: var(--text-primary);
	}

	/* Syntax highlighting - light mode */
	:global(.tiptap pre code .hljs-keyword),
	:global(.tiptap pre code .hljs-selector-tag),
	:global(.tiptap pre code .hljs-built_in) { color: #d73a49; }
	:global(.tiptap pre code .hljs-string),
	:global(.tiptap pre code .hljs-addition) { color: #032f62; }
	:global(.tiptap pre code .hljs-number),
	:global(.tiptap pre code .hljs-literal) { color: #005cc5; }
	:global(.tiptap pre code .hljs-comment),
	:global(.tiptap pre code .hljs-quote) { color: #6a737d; font-style: italic; }
	:global(.tiptap pre code .hljs-function),
	:global(.tiptap pre code .hljs-title) { color: #6f42c1; }
	:global(.tiptap pre code .hljs-type),
	:global(.tiptap pre code .hljs-title.class_) { color: #e36209; }
	:global(.tiptap pre code .hljs-variable),
	:global(.tiptap pre code .hljs-template-variable) { color: #e36209; }
	:global(.tiptap pre code .hljs-attr),
	:global(.tiptap pre code .hljs-attribute) { color: #005cc5; }
	:global(.tiptap pre code .hljs-tag) { color: #22863a; }
	:global(.tiptap pre code .hljs-name) { color: #22863a; }
	:global(.tiptap pre code .hljs-meta) { color: #005cc5; }
	:global(.tiptap pre code .hljs-deletion) { color: #b31d28; background: #ffeef0; }
	:global(.tiptap pre code .hljs-symbol),
	:global(.tiptap pre code .hljs-bullet) { color: #005cc5; }
	:global(.tiptap pre code .hljs-regexp) { color: #032f62; }
	:global(.tiptap pre code .hljs-params) { color: #24292e; }
	:global(.tiptap pre code .hljs-punctuation) { color: #24292e; }
	:global(.tiptap pre code .hljs-property) { color: #005cc5; }
	:global(.tiptap pre code .hljs-selector-class) { color: #6f42c1; }
	:global(.tiptap pre code .hljs-selector-id) { color: #005cc5; }
	:global(.tiptap pre code .hljs-operator) { color: #d73a49; }

	/* Syntax highlighting - dark mode */
	:global(.dark .tiptap pre code .hljs-keyword),
	:global(.dark .tiptap pre code .hljs-selector-tag),
	:global(.dark .tiptap pre code .hljs-built_in) { color: #ff7b72; }
	:global(.dark .tiptap pre code .hljs-string),
	:global(.dark .tiptap pre code .hljs-addition) { color: #a5d6ff; }
	:global(.dark .tiptap pre code .hljs-number),
	:global(.dark .tiptap pre code .hljs-literal) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-comment),
	:global(.dark .tiptap pre code .hljs-quote) { color: #8b949e; font-style: italic; }
	:global(.dark .tiptap pre code .hljs-function),
	:global(.dark .tiptap pre code .hljs-title) { color: #d2a8ff; }
	:global(.dark .tiptap pre code .hljs-type),
	:global(.dark .tiptap pre code .hljs-title.class_) { color: #ffa657; }
	:global(.dark .tiptap pre code .hljs-variable),
	:global(.dark .tiptap pre code .hljs-template-variable) { color: #ffa657; }
	:global(.dark .tiptap pre code .hljs-attr),
	:global(.dark .tiptap pre code .hljs-attribute) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-tag) { color: #7ee787; }
	:global(.dark .tiptap pre code .hljs-name) { color: #7ee787; }
	:global(.dark .tiptap pre code .hljs-meta) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-deletion) { color: #ffdcd7; background: #67060c; }
	:global(.dark .tiptap pre code .hljs-symbol),
	:global(.dark .tiptap pre code .hljs-bullet) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-regexp) { color: #a5d6ff; }
	:global(.dark .tiptap pre code .hljs-params) { color: #c9d1d9; }
	:global(.dark .tiptap pre code .hljs-punctuation) { color: #c9d1d9; }
	:global(.dark .tiptap pre code .hljs-property) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-selector-class) { color: #d2a8ff; }
	:global(.dark .tiptap pre code .hljs-selector-id) { color: #79c0ff; }
	:global(.dark .tiptap pre code .hljs-operator) { color: #ff7b72; }

	.math-modal-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 2200;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.math-modal {
		background: var(--bg-primary);
		border: 1px solid var(--border);
		border-radius: 8px;
		min-width: min(480px, 92vw);
		max-width: 80vw;
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
	}
	.math-modal-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-weight: 500;
		font-size: 14px;
		color: var(--text-primary);
	}
	.math-modal-close {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-secondary);
		padding: 4px;
		display: flex;
	}
	.math-modal-close:hover { color: var(--text-primary); }
	.math-modal-input {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 13px;
		min-height: 80px;
		padding: 10px;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		resize: vertical;
		outline: none;
	}
	.math-modal-input:focus { border-color: var(--accent); }
	.math-modal-preview {
		min-height: 60px;
		padding: 14px;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-secondary);
		display: flex;
		align-items: center;
		justify-content: center;
		overflow-x: auto;
		color: var(--text-primary);
	}
	.math-modal-preview-empty {
		color: var(--text-tertiary);
		font-size: 13px;
	}
	.math-modal-preview :global(.math-modal-preview-error) {
		color: #d32f2f;
		font-size: 12px;
		font-family: var(--font-mono, monospace);
	}
	.math-modal-footer {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
	}
	.math-modal-hint {
		font-size: 11px;
		color: var(--text-tertiary);
	}
	.math-modal-actions {
		display: flex;
		gap: 8px;
	}
	.math-modal-actions button {
		appearance: none;
		padding: 6px 14px;
		border-radius: 5px;
		cursor: pointer;
		font-size: 13px;
		font-family: inherit;
		border: 1px solid var(--border);
		background: var(--bg-secondary);
		color: var(--text-primary);
	}
	.math-modal-actions button:hover:not(:disabled) {
		border-color: var(--accent);
	}
	.math-modal-actions button.primary {
		background: var(--accent);
		color: var(--accent-fg, white);
		border-color: var(--accent);
	}
	.math-modal-actions button.primary:hover:not(:disabled) {
		filter: brightness(1.1);
	}
	.math-modal-actions button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.secret-modal {
		max-width: min(720px, 92vw);
	}

	.secret-modal-text {
		min-height: 140px;
	}

	.secret-modal-title {
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		padding: 9px 10px;
		font: inherit;
		outline: none;
	}

	.secret-modal-title:focus {
		border-color: var(--accent);
	}

	.secret-modal-fields {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}

	.secret-modal-fields input {
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		padding: 9px 10px;
		font: inherit;
		outline: none;
	}

	.secret-modal-fields input:focus {
		border-color: var(--accent);
	}

	.secret-modal-error {
		color: #d32f2f;
		font-size: 12px;
	}

	@media (max-width: 640px) {
		.secret-modal-fields {
			grid-template-columns: 1fr;
		}
	}

	.viewer-banner {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 14px;
		background: var(--bg-tertiary, var(--bg-secondary));
		border-bottom: 1px solid var(--border);
		font-size: 13px;
		color: var(--text-primary);
		flex-shrink: 0;
	}
	.viewer-banner-icon {
		flex-shrink: 0;
		color: var(--accent, var(--text-secondary));
	}
	.viewer-banner-label {
		font-weight: 500;
		flex-shrink: 0;
	}
	.viewer-banner-path {
		color: var(--text-secondary);
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		flex: 1;
		min-width: 0;
	}
	.viewer-banner-actions {
		display: flex;
		gap: 6px;
		flex-shrink: 0;
	}
	.viewer-banner-btn {
		appearance: none;
		background: var(--bg-primary);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 4px 12px;
		font-size: 12px;
		cursor: pointer;
		font-family: inherit;
	}
	.viewer-banner-btn:hover:not(:disabled) {
		border-color: var(--accent);
	}
	.viewer-banner-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.viewer-banner-btn.primary {
		background: var(--accent);
		color: var(--accent-fg, white);
		border-color: var(--accent);
	}
	.viewer-banner-btn.primary:hover:not(:disabled) {
		filter: brightness(1.1);
	}
	.viewer-banner-toast {
		font-size: 12px;
		color: var(--text-secondary);
		flex-shrink: 0;
	}

	.viewer-import-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		z-index: 2100;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.viewer-import-picker {
		background: var(--bg-primary);
		border: 1px solid var(--border);
		border-radius: 8px;
		min-width: 320px;
		max-width: 480px;
		max-height: 70vh;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.3);
	}
	.viewer-import-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
		font-weight: 500;
		font-size: 14px;
	}
	.viewer-import-close {
		appearance: none;
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 4px;
		display: flex;
	}
	.viewer-import-close:hover {
		color: var(--text-primary);
	}
	.viewer-import-list {
		overflow-y: auto;
		padding: 6px 0;
	}
	.viewer-import-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		appearance: none;
		background: none;
		border: none;
		color: var(--text-primary);
		padding: 8px 14px;
		text-align: left;
		font-size: 13px;
		cursor: pointer;
		font-family: inherit;
	}
	.viewer-import-item:hover:not(:disabled) {
		background: var(--bg-secondary);
	}
	.viewer-import-item:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.viewer-import-item svg {
		color: var(--text-tertiary);
		flex-shrink: 0;
	}

	.tag-menu-overlay {
		position: fixed;
		inset: 0;
		z-index: 2000;
	}

	.tag-menu {
		position: fixed;
		width: 240px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: var(--shadow-lg);
		padding: 8px;
		z-index: 2001;
	}

	.tag-menu-list {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-bottom: 8px;
	}

	.tag-menu-chip {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		color: var(--text-secondary);
		background: var(--bg-tertiary);
		padding: 2px 4px 2px 8px;
		border-radius: 10px;
	}

	.tag-menu-remove {
		display: flex;
		align-items: center;
		justify-content: center;
		border: none;
		background: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 1px;
		border-radius: 50%;
	}

	.tag-menu-remove:hover {
		color: var(--danger, #e53e3e);
		background: color-mix(in srgb, var(--danger, #e53e3e) 12%, transparent);
	}

	.code-lang-overlay {
		position: fixed;
		inset: 0;
		z-index: 2000;
	}

	.code-lang-dropdown {
		position: fixed;
		transform: translateX(-100%);
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		max-height: 320px;
		overflow-y: auto;
		min-width: 150px;
		z-index: 2001;
	}

	.code-lang-search {
		width: 100%;
		box-sizing: border-box;
		padding: 5px 8px;
		margin-bottom: 4px;
		border: 1px solid var(--border-color);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 12px;
		outline: none;
	}

	.code-lang-search:focus {
		border-color: var(--accent);
	}

	.code-lang-empty {
		padding: 8px;
		text-align: center;
		color: var(--text-tertiary);
		font-size: 12px;
	}

	.code-lang-dropdown::-webkit-scrollbar {
		width: 5px;
	}

	.code-lang-dropdown::-webkit-scrollbar-track {
		background: transparent;
	}

	.code-lang-dropdown::-webkit-scrollbar-thumb {
		background: var(--border-color);
		border-radius: 3px;
	}

	.code-lang-option {
		display: block;
		width: 100%;
		padding: 5px 10px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 12px;
		font-family: 'JetBrains Mono', 'Fira Code', monospace;
		cursor: pointer;
		border-radius: 4px;
		text-align: left;
	}

	.code-lang-option:hover {
		background: var(--bg-hover);
	}

	.code-lang-option.active {
		color: var(--text-accent);
		background: var(--accent-light);
	}

	:global(.tiptap-wrapper .tiptap blockquote) {
		border-left: 3px solid var(--accent);
		padding-left: 16px;
		margin: 1em 0;
		color: var(--text-secondary);
	}

	/* Callouts (Obsidian-style) */
	:global(.tiptap-wrapper .tiptap .callout) {
		--cc: var(--callout-note);
		margin: 1em 0;
		border: 1px solid rgba(var(--cc), 0.3);
		border-left: 3px solid rgb(var(--cc));
		border-radius: 8px;
		background: rgba(var(--cc), 0.06);
		overflow: hidden;
	}
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="note"]) { --cc: var(--callout-note); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="abstract"]) { --cc: var(--callout-abstract); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="info"]) { --cc: var(--callout-info); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="todo"]) { --cc: var(--callout-todo); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="tip"]) { --cc: var(--callout-tip); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="success"]) { --cc: var(--callout-success); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="question"]) { --cc: var(--callout-question); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="warning"]) { --cc: var(--callout-warning); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="failure"]) { --cc: var(--callout-failure); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="danger"]) { --cc: var(--callout-danger); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="bug"]) { --cc: var(--callout-bug); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="example"]) { --cc: var(--callout-example); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="quote"]) { --cc: var(--callout-quote); }
	:global(.tiptap-wrapper .tiptap .callout[data-callout-group="custom"]) { --cc: var(--callout-custom); }

	:global(.tiptap-wrapper .tiptap .callout-header) {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		font-weight: 600;
		color: rgb(var(--cc));
		background: rgba(var(--cc), 0.1);
		user-select: none;
	}
	:global(.tiptap-wrapper .tiptap .callout-icon),
	:global(.tiptap-wrapper .tiptap .callout-fold) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		flex: 0 0 auto;
		padding: 0;
		border: none;
		background: none;
		cursor: pointer;
		color: rgb(var(--cc));
		border-radius: 4px;
		transition: background 0.15s, transform 0.2s;
	}
	:global(.tiptap-wrapper .tiptap .callout-icon:hover),
	:global(.tiptap-wrapper .tiptap .callout-fold:hover) {
		background: rgba(var(--cc), 0.18);
	}
	:global(.tiptap-wrapper .tiptap .callout.is-folded .callout-fold) {
		transform: rotate(-90deg);
	}
	:global(.tiptap-wrapper .tiptap .callout-title) {
		flex: 1 1 auto;
		min-width: 0;
		border: none;
		background: none;
		outline: none;
		font: inherit;
		font-weight: 600;
		color: rgb(var(--cc));
		padding: 2px 0;
	}
	:global(.tiptap-wrapper .tiptap .callout-title::placeholder) {
		color: rgb(var(--cc));
		opacity: 0.7;
	}
	:global(.tiptap-wrapper .tiptap .callout-content) {
		padding: 8px 14px 10px;
	}
	:global(.tiptap-wrapper .tiptap .callout.is-folded .callout-content) {
		display: none;
	}
	:global(.tiptap-wrapper .tiptap .callout-content > p:first-child) { margin-top: 0; }
	:global(.tiptap-wrapper .tiptap .callout-content > p:last-child) { margin-bottom: 0; }

	/* Callout type picker (rendered into <body>) */
	:global(.callout-type-menu) {
		position: fixed;
		z-index: 9999;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 2px;
		max-height: 60vh;
		overflow: auto;
	}
	:global(.callout-type-option) {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border: none;
		background: none;
		border-radius: 5px;
		cursor: pointer;
		font: inherit;
		font-size: 13px;
		color: var(--text-primary);
		text-align: left;
		white-space: nowrap;
	}
	:global(.callout-type-option:hover) {
		background: var(--bg-hover);
	}
	:global(.callout-type-icon) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		flex: 0 0 auto;
	}
	:global(.callout-type-menu.is-custom) {
		display: block;
	}
	:global(.callout-type-custom-input) {
		width: 200px;
		max-width: 60vw;
		border: 1px solid var(--border-color);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		padding: 7px 9px;
		font: inherit;
		font-size: 13px;
		outline: none;
	}
	:global(.callout-type-custom-input:focus) {
		border-color: var(--accent);
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"]) {
		margin: 1em 0;
		border: 1px solid var(--border-color);
		border-radius: 8px;
		position: relative;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] > button) {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		position: absolute;
		top: 8px;
		left: 6px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-secondary);
		padding: 0;
		border-radius: 4px;
		transition: background 0.15s;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] > button:hover) {
		background: var(--bg-tertiary);
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] > button::after) {
		content: '▶';
		font-size: 10px;
		transition: transform 0.2s;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"].is-open > button::after) {
		transform: rotate(90deg);
	}

	@starting-style {
		:global(.tiptap-wrapper .tiptap [data-type="details"].is-open > button::after) {
			transform: rotate(0deg);
		}
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] summary) {
		padding: 10px 14px 10px 32px;
		font-weight: 600;
		background: var(--bg-secondary);
		transition: background 0.15s;
		list-style: none;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] summary::-webkit-details-marker) {
		display: none;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] summary:hover) {
		background: var(--bg-tertiary);
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] [data-type="detailsContent"]) {
		padding: 10px 14px;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] [data-type="detailsContent"] > p:first-child) {
		margin-top: 0;
	}

	:global(.tiptap-wrapper .tiptap [data-type="details"] [data-type="detailsContent"] > p:last-child) {
		margin-bottom: 0;
	}

	:global(.tiptap-wrapper .tiptap ul),
	:global(.tiptap-wrapper .tiptap ol) {
		padding-left: 24px;
		margin: 0.5em 0;
	}

	:global(.tiptap-wrapper .tiptap ul:not([data-type="taskList"])) {
		list-style-type: disc;
	}

	:global(.tiptap-wrapper .tiptap ul:not([data-type="taskList"]) ul) {
		list-style-type: circle;
	}

	:global(.tiptap-wrapper .tiptap ul:not([data-type="taskList"]) ul ul) {
		list-style-type: square;
	}

	/* Counter-based numbering: WebKitGTK clips the native ol marker (overflow:hidden), so 11 rendered as 1. list-item respects <ol start> + nesting. */
	:global(.tiptap-wrapper .tiptap ol) {
		list-style: none;
		padding-left: 0;
	}
	:global(.tiptap-wrapper .tiptap ol > li) {
		position: relative;
		padding-left: 2.4em;
	}
	:global(.tiptap-wrapper .tiptap ol > li::before) {
		content: counter(list-item) ".";
		position: absolute;
		left: 0;
		width: 2em;
		text-align: right;
		white-space: nowrap;
	}
	:global(.tiptap-wrapper .tiptap ol ol > li::before) {
		content: counter(list-item, lower-alpha) ".";
	}
	:global(.tiptap-wrapper .tiptap ol ol ol > li::before) {
		content: counter(list-item, lower-roman) ".";
	}

	:global(.tiptap-wrapper .tiptap li) {
		margin: 0.25em 0;
		line-height: var(--editor-line-height, 1.65);
	}

	:global(.tiptap-wrapper .tiptap li p) {
		margin: 0;
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"]) {
		list-style: none;
		padding-left: 0;
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li) {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		margin: 4px 0;
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li label) {
		display: flex;
		align-items: center;
		margin-top: 4px;
		flex-shrink: 0;
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li label input[type="checkbox"]) {
		appearance: none;
		-webkit-appearance: none;
		width: 16px;
		height: 16px;
		border: 2px solid var(--border-color);
		border-radius: 4px;
		cursor: pointer;
		position: relative;
		flex-shrink: 0;
		background: var(--bg-primary);
		transition: background 0.15s, border-color 0.15s;
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li[data-checked="true"] label input[type="checkbox"]) {
		background: var(--accent);
		border-color: var(--accent);
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li[data-checked="true"] label input[type="checkbox"]::after) {
		content: '';
		position: absolute;
		left: 3px;
		top: 0px;
		width: 6px;
		height: 10px;
		border: solid white;
		border-width: 0 2px 2px 0;
		transform: rotate(45deg);
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li label input[type="checkbox"]:hover) {
		border-color: var(--accent);
	}

	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li > div) {
		flex: 1;
		min-width: 0;
	}

	/* Strike through only the direct paragraph content of a checked task item.
	   Using > p instead of > div prevents text-decoration from bleeding into
	   nested task lists, since CSS text-decoration cannot be cancelled by descendants. */
	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li[data-checked="true"] > div > p) {
		text-decoration: line-through;
		color: var(--text-tertiary);
	}

	/* Dim inline task metadata (!high / due:YYYY-MM-DD) so it recedes but stays readable. */
	:global(.tiptap-wrapper .tiptap .task-meta-dim) {
		opacity: 0.45;
	}

	:global(.tiptap-wrapper .tiptap hr) {
		border: none;
		border-top: 1px solid var(--border-color);
		margin: 1.5em 0;
	}

	:global(.tiptap-wrapper .tiptap a) {
		color: var(--text-accent);
		text-decoration: underline;
		text-decoration-color: color-mix(in srgb, var(--text-accent) 40%, transparent);
	}

	:global(.tiptap-wrapper .tiptap a::after) {
		content: '↗';
		display: inline;
		font-size: 0.65em;
		margin-left: 2px;
		opacity: 0.5;
		vertical-align: 15%;
	}

	:global(html.no-link-arrows .tiptap-wrapper .tiptap a::after) {
		content: none;
	}

	:global(html.no-link-arrows .tiptap-wrapper .tiptap a[href$=".md"]::after) {
		content: none;
	}

	:global(.tiptap-wrapper .tiptap a[href$=".md"]::after) {
		content: '⤴';
	}

	:global(.tiptap-wrapper .tiptap a:hover) {
		text-decoration-color: var(--text-accent);
	}

	:global(.tiptap-wrapper .tiptap a:hover::after) {
		opacity: 0.8;
	}

	:global(.tiptap-wrapper .tiptap img) {
		display: block;
		max-width: 100%;
		height: auto;
		border-radius: 8px;
		margin: 1em 0;
		cursor: pointer;
	}

	:global(.tiptap-wrapper .tiptap img:hover) {
		outline: none;
	}

	:global(.tiptap-wrapper .tiptap img.ProseMirror-selectednode) {
		outline: none !important;
		box-shadow: none !important;
		background: none !important;
	}

	:global(.tiptap-wrapper .tiptap .ProseMirror-selectednode) {
		outline: none !important;
	}

	:global(.tiptap-wrapper .tiptap img::selection) {
		background: transparent;
	}

	:global(.tiptap-wrapper .tiptap img::-moz-selection) {
		background: transparent;
	}

	:global(.tiptap-wrapper .tiptap img[data-size="small"]) {
		max-width: 33%;
	}

	:global(.tiptap-wrapper .tiptap img[data-size="medium"]) {
		max-width: 65%;
	}

	:global(.tiptap-wrapper .tiptap img[data-size="full"]) {
		max-width: 100%;
	}

	.img-toolbar-overlay {
		position: fixed;
		inset: 0;
		z-index: 1500;
	}

	.img-toolbar {
		position: fixed;
		display: flex;
		align-items: center;
		gap: 2px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 8px;
		box-shadow: var(--shadow-lg);
		padding: 3px;
		z-index: 1501;
	}

	.img-toolbar button {
		padding: 5px 14px;
		border: none;
		background: none;
		color: var(--text-secondary);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		border-radius: 5px;
	}

	.img-toolbar button:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.img-toolbar button.active {
		background: var(--accent);
		color: white;
	}

	.img-toolbar-sep {
		width: 1px;
		height: 16px;
		background: var(--border-color);
		margin: 0 2px;
	}

	.img-toolbar button svg {
		display: block;
	}

	.copy-toast {
		position: fixed;
		bottom: 24px;
		right: 24px;
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 16px;
		min-width: 100px;
		justify-content: center;
		background: var(--accent);
		border-radius: 8px;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
		font-size: 13px;
		font-weight: 500;
		color: white;
		z-index: 9999;
		animation: toast-in 0.15s ease-out;
	}

	.copy-toast.done {
		background: var(--accent);
	}

	.copy-toast-spinner {
		animation: copy-spin 0.8s linear infinite;
	}

	@keyframes toast-in {
		from { opacity: 0; transform: translateY(8px); }
		to { opacity: 1; transform: translateY(0); }
	}

	@keyframes copy-spin {
		to { transform: rotate(360deg); }
	}

	:global(.tiptap-wrapper .tiptap mark) {
		padding: 0px 5px 2px;
		border-radius: 3px;
		color: #333 !important;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
	}

	:global(.dark .tiptap-wrapper .tiptap mark) {
		color: #eee !important;
	}

	:global(.tiptap-wrapper .tiptap .tableWrapper) {
		overflow-x: auto;
		margin: 1em 0;
	}

	:global(.tiptap-wrapper .tiptap table) {
		border-collapse: collapse;
		width: 100%;
		table-layout: fixed;
		overflow: hidden;
	}

	:global(.tiptap-wrapper .tiptap th),
	:global(.tiptap-wrapper .tiptap td) {
		border: 1px solid var(--border-color);
		padding: 8px 12px;
		text-align: left;
		vertical-align: top;
		position: relative;
		min-width: 80px;
		box-sizing: border-box;
	}

	:global(.tiptap-wrapper .tiptap th) {
		background: var(--bg-tertiary);
		font-weight: 600;
	}

	:global(.tiptap-wrapper .tiptap td > p),
	:global(.tiptap-wrapper .tiptap th > p) {
		margin: 0;
	}

	:global(.tiptap-wrapper .tiptap .selectedCell::after) {
		content: "";
		position: absolute;
		inset: 0;
		background: var(--accent-light);
		pointer-events: none;
		z-index: 1;
	}

	:global(.tiptap-wrapper .tiptap .column-resize-handle) {
		position: absolute;
		right: -2px;
		top: 0;
		bottom: -2px;
		width: 4px;
		background: var(--accent);
		pointer-events: none;
		z-index: 2;
	}

	:global(.tiptap-wrapper .tiptap.resize-cursor) {
		cursor: col-resize;
	}

	:global(.tiptap-wrapper .tiptap > .is-empty::before) {
		content: attr(data-placeholder);
		color: var(--text-tertiary);
		pointer-events: none;
		float: left;
		height: 0;
		padding-left: 2px;
	}

	/* Suppress placeholder on task list / details / callout containers - it overlaps their own UI (checkbox, toggle, callout header) */
	:global(.tiptap-wrapper .tiptap > ul[data-type="taskList"].is-empty::before),
	:global(.tiptap-wrapper .tiptap > [data-type="details"].is-empty::before),
	:global(.tiptap-wrapper .tiptap > .callout.is-empty::before) {
		content: none;
	}

	/* Show placeholder on the paragraph inside task item content div */
	:global(.tiptap-wrapper .tiptap ul[data-type="taskList"] li > div > p.is-empty::before) {
		content: attr(data-placeholder);
		color: var(--text-tertiary);
		pointer-events: none;
		float: left;
		height: 0;
		padding-left: 2px;
	}

	/* Show placeholder on paragraphs inside collapsible section summary and content */
	:global(.tiptap-wrapper .tiptap [data-type="detailsSummary"] p.is-empty::before),
	:global(.tiptap-wrapper .tiptap [data-type="detailsContent"] p.is-empty::before) {
		content: attr(data-placeholder);
		color: var(--text-tertiary);
		pointer-events: none;
		float: left;
		height: 0;
		padding-left: 2px;
	}


	.link-context-overlay {
		position: fixed;
		inset: 0;
		z-index: 1500;
	}

	.link-context-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 180px;
		z-index: 1501;
	}

	.link-context-url {
		padding: 6px 12px;
		font-size: 11px;
		color: var(--text-tertiary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 240px;
		border-bottom: 1px solid var(--border-light);
		margin-bottom: 4px;
	}

	.link-context-menu button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.link-context-menu button:hover {
		background: var(--bg-hover);
	}

	.link-context-menu button.danger {
		color: var(--danger);
	}

	.link-context-menu button.danger:hover {
		background: color-mix(in srgb, var(--danger) 10%, transparent);
	}

	.link-context-sep {
		height: 1px;
		background: var(--border-light);
		margin: 4px 0;
	}

	.link-modal-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 2000;
	}

	.link-modal {
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 12px;
		box-shadow: var(--shadow-lg);
		padding: 20px;
		width: 400px;
		max-width: 90vw;
	}

	.link-modal-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 16px;
		font-size: 15px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.link-modal-input {
		width: 100%;
		padding: 10px 12px;
		border: 1px solid var(--border-color);
		border-radius: 8px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 14px;
		outline: none;
		box-sizing: border-box;
	}

	.link-modal-input:focus {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-light);
	}

	.link-modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 16px;
	}

	.link-modal-btn {
		padding: 7px 16px;
		border-radius: 8px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		border: none;
	}

	.link-modal-btn.cancel {
		background: var(--bg-tertiary);
		color: var(--text-secondary);
	}

	.link-modal-btn.cancel:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.link-modal-btn.confirm {
		background: var(--accent);
		color: white;
	}

	.link-modal-btn.confirm:hover {
		opacity: 0.9;
	}

	.link-suggest-list {
		max-height: 240px;
		overflow-y: auto;
		margin-top: 8px;
		border: 1px solid var(--border-light);
		border-radius: 8px;
		padding: 4px;
	}

	.link-suggest-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.link-suggest-item:hover,
	.link-suggest-item.selected {
		background: var(--accent-light);
		color: var(--accent);
	}

	.link-suggest-item svg {
		flex-shrink: 0;
		color: var(--text-tertiary);
	}

	.link-suggest-item:hover svg,
	.link-suggest-item.selected svg {
		color: var(--accent);
	}

	.link-suggest-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Text context menu */
	.text-ctx-overlay {
		position: fixed;
		inset: 0;
		z-index: 1500;
	}

	.text-ctx-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 200px;
		z-index: 1501;
	}

	.text-ctx-menu button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.text-ctx-menu button:hover {
		background: var(--bg-hover);
	}

	.text-ctx-shortcut {
		margin-left: auto;
		font-size: 11px;
		color: var(--text-tertiary);
		font-family: inherit;
	}

	.text-ctx-sep {
		height: 1px;
		background: var(--border-light);
		margin: 4px 0;
	}

	.text-ctx-submenu-wrap {
		position: relative;
	}

	.text-ctx-submenu-wrap > button.has-submenu {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.text-ctx-submenu-wrap > button.has-submenu:hover {
		background: var(--bg-hover);
	}

	.text-ctx-submenu-wrap .submenu-arrow {
		margin-left: auto;
		color: var(--text-tertiary);
	}

	.text-ctx-submenu {
		position: absolute;
		left: 100%;
		top: -4px;
		margin-left: 2px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 140px;
		z-index: 1502;
	}

	.text-ctx-submenu.flip-left {
		left: auto;
		right: 100%;
		margin-left: 0;
		margin-right: 2px;
	}

	.text-ctx-submenu button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.text-ctx-submenu button:hover {
		background: var(--bg-hover);
	}

	.text-ctx-submenu button.active {
		color: var(--accent);
		font-weight: 600;
	}

	/* Table grid size picker */
	.table-picker-dropdown {
		padding: 8px;
		min-width: auto;
		width: auto;
	}

	.table-picker-grid {
		display: grid;
		grid-template-columns: repeat(10, 18px);
		grid-template-rows: repeat(8, 18px);
		gap: 2px;
	}

	.table-picker-cell {
		width: 18px;
		height: 18px;
		border: 1px solid var(--border-color);
		border-radius: 2px;
		cursor: pointer;
		transition: background 0.05s;
		background: var(--bg-secondary);
	}

	.table-picker-cell:hover,
	.table-picker-cell.active {
		background: var(--accent-light);
		border-color: var(--accent);
	}

	.table-picker-label {
		text-align: center;
		font-size: 11px;
		color: var(--text-secondary);
		margin-top: 6px;
		font-weight: 500;
	}

	/* Table context menu */
	.table-ctx-overlay {
		position: fixed;
		inset: 0;
		z-index: 1500;
	}

	.table-ctx-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 200px;
		max-height: calc(100vh - 16px);
		overflow-y: auto;
		z-index: 1501;
	}

	.table-ctx-menu button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.table-ctx-menu button:hover {
		background: var(--bg-hover);
	}

	.table-ctx-menu button.danger {
		color: var(--danger);
	}

	.table-ctx-menu button.danger:hover {
		background: color-mix(in srgb, var(--danger) 10%, transparent);
	}

	.table-ctx-sep {
		height: 1px;
		background: var(--border-light);
		margin: 4px 0;
	}

	.table-ctx-color-label {
		padding: 4px 12px 2px;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-tertiary);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.table-ctx-colors {
		display: grid;
		grid-template-columns: repeat(5, 1fr);
		gap: 4px;
		padding: 4px 10px 6px;
	}

	.table-ctx-color-swatch {
		width: 26px;
		height: 26px;
		border-radius: 5px;
		border: 1px solid var(--border-color);
		cursor: pointer;
		transition: transform 0.1s;
		padding: 0;
	}

	.table-ctx-color-swatch:hover {
		transform: scale(1.15);
		border-color: var(--accent);
	}

	/* PDF embeds */
	:global(.tiptap .math-block) {
		margin: 16px 0;
		padding: 12px;
		text-align: center;
		overflow-x: auto;
		cursor: default;
	}
	:global(.tiptap .math-inline) {
		cursor: default;
	}
	:global(.tiptap .mermaid-render) {
		position: relative;
		margin: 4px 0 16px;
		padding: 12px;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 8px;
		text-align: center;
		overflow-x: auto;
		cursor: default;
		min-height: 32px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}
	:global(.tiptap .mermaid-render-toolbar) {
		position: absolute;
		top: 6px;
		right: 6px;
		display: flex;
		gap: 4px;
		opacity: 0;
		transition: opacity 0.15s ease;
	}
	:global(.tiptap .mermaid-render:hover .mermaid-render-toolbar) {
		opacity: 1;
	}
	:global(.tiptap .mermaid-render-action) {
		appearance: none;
		background: var(--bg-primary);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 5px;
		padding: 3px 9px;
		font-size: 11px;
		cursor: pointer;
		font-family: inherit;
		line-height: 1.3;
	}
	:global(.tiptap .mermaid-render-action:hover) {
		color: var(--text-primary);
		border-color: var(--accent);
	}
	:global(.tiptap .mermaid-render-toast) {
		position: absolute;
		bottom: 8px;
		right: 8px;
		background: var(--text-primary);
		color: var(--bg-primary);
		padding: 4px 10px;
		border-radius: 5px;
		font-size: 12px;
		max-width: 80%;
		pointer-events: none;
		animation: mermaid-toast-fade 1.5s ease;
	}
	@keyframes mermaid-toast-fade {
		0% { opacity: 0; transform: translateY(4px); }
		15% { opacity: 1; transform: translateY(0); }
		70% { opacity: 1; }
		100% { opacity: 0; }
	}
	:global(.tiptap .mermaid-render svg) {
		max-width: 100%;
		height: auto;
	}
	:global(.tiptap .mermaid-render-loading::before) {
		content: 'Rendering diagram…';
		color: var(--text-secondary);
		font-size: 13px;
	}
	:global(.tiptap .mermaid-render-error) {
		color: var(--text-secondary);
		font-size: 13px;
	}
	:global(.tiptap .mermaid-render-btn) {
		appearance: none;
		background: var(--bg-tertiary, var(--bg-primary));
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 6px 14px;
		font-size: 13px;
		cursor: pointer;
		font-family: inherit;
	}
	:global(.tiptap .mermaid-render-btn:hover) {
		background: var(--accent);
		color: var(--accent-fg, white);
		border-color: var(--accent);
	}
	:global(.tiptap .mermaid-render-btn-small) {
		padding: 3px 10px;
		font-size: 12px;
	}
	:global(.tiptap .pdf-embed) {
		margin: 12px 0;
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
		background: var(--bg-secondary);
	}
	:global(.tiptap .pdf-embed iframe) {
		display: block;
		border: none;
		width: 100%;
	}
	:global(.tiptap .pdf-embed .pdf-label) {
		padding: 6px 12px;
		font-size: 12px;
		color: var(--text-secondary);
		border-top: 1px solid var(--border);
		margin: 0;
	}
	:global(.tiptap .page-break) {
		position: relative;
		margin: 12px 0;
		height: 20px;
		border: none;
		pointer-events: none;
		user-select: none;
	}
	:global(.tiptap .page-break::before) {
		content: '';
		position: absolute;
		top: 50%;
		left: 0;
		right: 0;
		height: 0;
		border-top: 2px dashed var(--text-tertiary, #aaa);
		opacity: 0.5;
	}
	:global(.tiptap .page-break::after) {
		content: 'Page Break';
		position: absolute;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		background: var(--bg-primary, #fff);
		padding: 0 8px;
		font-size: 11px;
		color: var(--text-tertiary, #aaa);
		white-space: nowrap;
	}
	:global(.tiptap .pdf-embed-mobile) {
		margin: 8px 0;
	}
	:global(.tiptap .pdf-link-mobile) {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 16px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		color: var(--text-primary);
		font-size: 15px;
		font-weight: 500;
		text-decoration: none;
		cursor: pointer;
	}
	:global(.tiptap .pdf-link-mobile:active) {
		background: var(--bg-hover);
	}
	:global(.tiptap .pdf-icon-mobile) {
		font-size: 20px;
		line-height: 1;
	}

	/* Slash commands menu */
	.slash-menu-overlay {
		position: fixed;
		inset: 0;
		z-index: 1500;
	}

	.task-due-input {
		position: fixed;
		z-index: 1501;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		padding: 6px 8px;
		color: var(--text-primary);
		font: inherit;
		box-shadow: var(--shadow-lg);
	}

	.slash-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 220px;
		max-height: 300px;
		overflow-y: auto;
		z-index: 1501;
	}

	.slash-menu::-webkit-scrollbar {
		width: 4px;
	}

	.slash-menu::-webkit-scrollbar-thumb {
		background: var(--text-tertiary);
		border-radius: 2px;
	}

	.slash-menu-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 8px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.slash-menu-item:hover,
	.slash-menu-item.selected {
		background: var(--bg-hover);
	}

	.slash-menu-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent);
		flex-shrink: 0;
	}

	.slash-menu-label {
		flex: 1;
	}

	.slash-menu-empty {
		padding: 12px 16px;
		color: var(--text-tertiary);
		font-size: 13px;
		text-align: center;
	}

	.slash-table-picker {
		padding: 8px;
	}

	.slash-table-picker-grid {
		display: grid;
		grid-template-columns: repeat(10, 18px);
		grid-template-rows: repeat(8, 18px);
		gap: 2px;
	}

	.slash-table-picker-label {
		text-align: center;
		font-size: 11px;
		color: var(--text-secondary);
		margin-top: 6px;
		font-weight: 500;
	}

	/* Color swatch preview shown before a color literal (VSCode-style decorator) */
	:global(.tiptap-wrapper .tiptap .color-swatch) {
		display: inline-block;
		width: 0.8em;
		height: 0.8em;
		border-radius: 3px;
		margin-right: 0.3em;
		vertical-align: baseline;
		border: 1px solid var(--border-color);
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
		user-select: none;
	}

	/* /color slash sub-picker */
	.slash-color-picker {
		padding: 8px;
		width: 210px;
	}
	.slash-color-swatches {
		display: grid;
		grid-template-columns: repeat(6, 1fr);
		gap: 6px;
		margin-bottom: 8px;
	}
	.slash-color-swatch {
		width: 100%;
		aspect-ratio: 1;
		border-radius: 5px;
		border: 1px solid var(--border-color);
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
		cursor: pointer;
		padding: 0;
	}
	.slash-color-swatch:hover {
		transform: scale(1.1);
	}
	.slash-color-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.slash-color-native {
		width: 28px;
		height: 28px;
		padding: 0;
		border: 1px solid var(--border-color);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		flex-shrink: 0;
	}
	.slash-color-input {
		flex: 1;
		min-width: 0;
		padding: 6px 8px;
		border: 1px solid var(--border-color);
		border-radius: 6px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 12px;
		font-family: monospace;
		outline: none;
	}
	.slash-color-insert {
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: var(--accent);
		color: #fff;
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		flex-shrink: 0;
	}

	/* AI Menu */
	.ai-menu-overlay {
		position: fixed;
		inset: 0;
		z-index: 1600;
	}

	.ai-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		padding: 4px;
		min-width: 320px;
		max-width: 480px;
		z-index: 1601;
		max-height: 80vh;
		overflow-y: auto;
	}

	.ai-menu-label {
		padding: 6px 12px 4px;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-tertiary);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.ai-menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 6px;
		text-align: left;
	}

	.ai-menu-item:hover {
		background: var(--bg-hover);
	}

	.ai-menu-sep {
		height: 1px;
		background: var(--border-light);
		margin: 4px 0;
	}

	.ai-menu-arrow {
		margin-left: auto;
		color: var(--text-tertiary);
		display: flex;
		align-items: center;
	}

	.ai-result-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px 4px;
	}

	.ai-result-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-secondary);
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.ai-spinner {
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.ai-result-close {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.ai-result-close:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.ai-result-body {
		padding: 8px 12px;
		font-size: 13px;
		color: var(--text-primary);
		line-height: 1.6;
		max-height: 50vh;
		overflow-y: auto;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.ai-error {
		padding: 8px 12px;
		font-size: 12px;
		color: var(--danger);
		line-height: 1.5;
	}

	.ai-result-actions {
		display: flex;
		gap: 6px;
		padding: 6px 10px 8px;
		border-top: 1px solid var(--border-light);
	}

	.ai-action-btn {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.15s;
	}

	.ai-action-btn.apply {
		background: var(--accent);
		color: white;
	}

	.ai-action-btn.apply:hover {
		opacity: 0.9;
	}

	.ai-action-btn.discard {
		background: var(--bg-secondary);
		border: 1px solid var(--border-color);
		color: var(--text-secondary);
	}

	.ai-action-btn.discard:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.ai-custom-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px 4px;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-secondary);
	}

	.ai-back-btn {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.ai-back-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.ai-custom-body {
		padding: 6px 10px 8px;
	}

	.ai-custom-input {
		width: 100%;
		min-height: 60px;
		padding: 8px 10px;
		border: 1px solid var(--border-color);
		border-radius: 6px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 13px;
		font-family: inherit;
		resize: vertical;
		outline: none;
		box-sizing: border-box;
	}

	.ai-custom-input:focus {
		border-color: var(--accent);
	}

	.ai-custom-input::placeholder {
		color: var(--text-tertiary);
	}

	.ai-custom-submit {
		margin-top: 6px;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 4px;
		width: 100%;
		padding: 7px 12px;
		border: none;
		border-radius: 6px;
		background: var(--accent);
		color: white;
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
	}

	.ai-custom-submit:hover:not(:disabled) {
		opacity: 0.9;
	}

	.ai-custom-submit:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* Wiki-link styles */
	:global(.tiptap-wrapper .tiptap .wiki-link) {
		color: var(--accent);
		text-decoration: underline dotted;
		text-underline-offset: 3px;
		cursor: pointer;
		border-radius: 2px;
		padding: 0 1px;
	}

	:global(.tiptap-wrapper .tiptap .wiki-link:hover) {
		background: var(--accent-light);
		text-decoration: underline solid;
	}

	:global(.tiptap-wrapper .tiptap .wiki-link[data-path=""]) {
		color: var(--text-tertiary);
		text-decoration: underline dashed;
	}

	.wiki-link-overlay {
		position: fixed;
		inset: 0;
		z-index: 1600;
	}

	.wiki-link-menu {
		position: fixed;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: var(--shadow-lg);
		width: 280px;
		max-height: 360px;
		overflow-y: auto;
		padding: 4px;
		z-index: 1601;
	}

	.wiki-link-empty {
		padding: 12px 14px;
		font-size: 12px;
		color: var(--text-tertiary);
		text-align: center;
	}

	.wiki-link-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		border-radius: 6px;
		background: none;
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
	}

	.wiki-link-item:hover,
	.wiki-link-item.selected {
		background: var(--accent-light);
		color: var(--accent);
	}

	.wiki-link-item svg {
		flex-shrink: 0;
		color: var(--text-tertiary);
	}

	.wiki-link-item:hover svg,
	.wiki-link-item.selected svg {
		color: var(--accent);
	}

	.wiki-link-title-col {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-width: 0;
	}

	.wiki-link-title {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.wiki-link-folder {
		font-size: 11px;
		color: var(--text-tertiary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.wiki-link-disambig-header {
		padding: 8px 10px 4px;
		font-size: 11px;
		color: var(--text-tertiary);
		font-weight: 500;
	}

	/* ═══ MOBILE (class-based, not media-query, for Android high-DPI) ═══ */
	.editor-container.mobile {
		height: 100%;
		min-height: 0;
		overflow: hidden;
	}

	.editor-container.mobile .editor-toolbar {
		padding: 8px 16px 6px 16px;
		flex-shrink: 0;
		flex-direction: column;
		align-items: stretch;
		gap: 2px;
	}

	.toolbar-actions.mobile {
		gap: 4px;
	}

	.toolbar-actions.mobile .icon-btn {
		min-width: 32px;
		min-height: 32px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.toolbar-actions.mobile .save-indicator,
	.toolbar-actions.mobile .readonly-indicator {
		font-size: 12px;
	}

	.editor-container.mobile .editor-title input {
		font-size: 20px;
		padding: 4px 0;
	}

	.editor-container.mobile .editor-body-wrapper {
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.editor-container.mobile .editor-body-row {
		min-height: 0;
	}

	.editor-container.mobile .editor-body {
		min-height: 0;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 0;
	}

	.editor-container.mobile .tiptap-wrapper {
		min-height: 0;
	}

	.editor-container.mobile .editor-formatting-bar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		z-index: 50;
		padding: 4px 8px;
		padding-bottom: calc(4px + env(safe-area-inset-bottom));
		overflow-x: auto;
		flex-wrap: nowrap;
		-webkit-overflow-scrolling: touch;
		scrollbar-width: none;
		gap: 1px;
		background: var(--bg-secondary);
		border-top: 1px solid var(--border-color);
	}

	.editor-container.mobile .editor-formatting-bar::-webkit-scrollbar {
		display: none;
	}

	.editor-container.mobile .fmt-btn {
		min-width: 38px;
		height: 38px;
		flex-shrink: 0;
		padding: 6px;
	}

	.editor-container.mobile .fmt-sep {
		height: 20px;
		margin: 0 2px;
	}

	.editor-container.mobile .fmt-dropdown {
		position: absolute;
		bottom: calc(100% + 4px);
		left: 0;
		right: auto;
		min-width: 180px;
		max-width: calc(100vw - 16px);
		max-height: 60vh;
		overflow-y: auto;
	}

	.editor-container.mobile .fmt-dropdown button {
		padding: 12px 16px;
		font-size: 15px;
		min-height: 44px;
	}

	.editor-container.mobile .insert-dropdown {
		position: absolute;
		bottom: calc(100% + 4px);
		left: 0;
		right: auto;
		min-width: 200px;
		max-width: calc(100vw - 16px);
	}

	.editor-container.mobile .shortcuts-hint {
		display: none;
	}

	.editor-container.mobile .empty-editor p {
		font-size: 16px;
	}

	.editor-container.mobile :global(.editor-content) {
		padding: 8px 16px 220px !important;
		/* Respect the user's font-size setting; 16px is the default. (issue #100)
		   Auto-zoom isn't a concern: the viewport sets user-scalable=no. */
		font-size: var(--editor-font-size, 16px) !important;
	}

	.editor-container.mobile .source-editor {
		padding: 8px 16px 220px;
		font-size: var(--editor-font-size, 15px);
		white-space: pre-wrap;
		word-break: break-word;
		overflow-x: hidden;
	}

	.editor-container.mobile .editor-body-row {
		position: relative;
	}

	.editor-container.mobile .editor-body-row:has(.history-panel) > .editor-body,
	.editor-container.mobile .editor-body-row:has(.outline-panel) > .editor-body {
		display: none;
	}

	.editor-container.mobile .history-panel,
	.editor-container.mobile .outline-panel {
		position: static;
		flex: 1;
		min-height: 0;
		width: 100% !important;
		max-width: 100%;
		border-left: none;
	}

	.editor-container.mobile .history-item {
		padding: 12px 12px;
		min-height: 48px;
	}

	.editor-container.mobile .history-list {
		padding-bottom: 80px;
	}

	.editor-container.mobile .history-restore-btn {
		padding: 14px 16px;
		font-size: 15px;
	}

	.editor-container.mobile .history-actions {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		padding: 12px;
		padding-bottom: calc(12px + env(safe-area-inset-bottom, 0px));
		background: var(--bg-secondary);
		border-top: 1px solid var(--border-light);
		z-index: 51;
	}

	.editor-container.mobile .note-search-bar {
		padding: 8px 12px;
	}

	.editor-container.mobile .note-search-bar input {
		padding: 8px 10px;
		font-size: 15px;
	}

	/* ═══ AI Menu - Mobile Bottom Sheet ═══ */
	.ai-menu-overlay.mobile {
		background: rgba(0, 0, 0, 0.35);
		display: flex;
		align-items: flex-end;
		justify-content: center;
	}

	.ai-menu.mobile {
		position: relative;
		left: auto !important;
		top: auto !important;
		width: 100%;
		max-width: 100%;
		min-width: 0;
		border-radius: 16px 16px 0 0;
		border-bottom: none;
		max-height: 70vh;
		padding: 8px 4px calc(env(safe-area-inset-bottom, 0px) + 8px);
	}

	.ai-menu.mobile .ai-menu-label {
		padding: 10px 16px 6px;
		font-size: 12px;
	}

	.ai-menu.mobile .ai-menu-item {
		padding: 12px 16px;
		font-size: 15px;
		min-height: 44px;
		border-radius: 8px;
	}

	.ai-menu.mobile .ai-menu-sep {
		margin: 4px 8px;
	}

	.ai-menu.mobile .ai-result-header {
		padding: 12px 16px 8px;
	}

	.ai-menu.mobile .ai-result-body {
		padding: 8px 16px;
		font-size: 15px;
		max-height: 40vh;
	}

	.ai-menu.mobile .ai-result-actions {
		padding: 8px 16px 4px;
		gap: 10px;
	}

	.ai-menu.mobile .ai-result-actions button {
		padding: 10px 16px;
		font-size: 14px;
		min-height: 44px;
	}

	.ai-menu.mobile .ai-custom-body {
		padding: 8px 12px;
	}

	.ai-menu.mobile .ai-custom-input {
		font-size: 15px;
		min-height: 80px;
	}

	.ai-menu.mobile .ai-custom-submit {
		padding: 10px 16px;
		font-size: 14px;
		min-height: 44px;
	}

	.ai-menu.mobile .ai-custom-header {
		padding: 10px 12px;
		font-size: 15px;
	}

	.ai-menu.mobile .ai-back-btn {
		min-width: 44px;
		min-height: 44px;
	}

	.ai-menu.mobile .ai-error {
		padding: 12px 16px;
		font-size: 14px;
	}

	/* ═══ Info Panel ═══ */
	.info-panel {
		width: 260px;
		border-left: 1px solid var(--border-light);
		background: var(--bg-secondary);
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		flex-shrink: 0;
	}

	.info-panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 14px;
		border-bottom: 1px solid var(--border-light);
		flex-shrink: 0;
	}

	.info-panel-title {
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-tertiary);
	}

	.info-close-btn {
		background: none;
		border: none;
		color: var(--text-tertiary);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.info-close-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.info-section {
		padding: 12px 14px;
		border-bottom: 1px solid var(--border-light);
	}

	.info-section:last-child {
		border-bottom: none;
		flex: 1;
	}

	.info-section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}

	.info-section-label {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-tertiary);
		margin-bottom: 8px;
	}

	.info-section-header .info-section-label {
		margin-bottom: 0;
	}

	.info-snapshot-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		background: none;
		border: 1px solid var(--border-color);
		border-radius: 4px;
		color: var(--text-secondary);
		cursor: pointer;
		padding: 3px 7px;
		font-size: 11px;
	}

	.info-snapshot-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.info-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		padding: 3px 0;
	}

	.info-row-tags {
		align-items: flex-start;
	}

	.info-key {
		font-size: 12px;
		color: var(--text-tertiary);
		flex-shrink: 0;
	}

	.info-value {
		font-size: 12px;
		color: var(--text-primary);
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.info-value-path {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 140px;
	}

	.info-tags {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		justify-content: flex-end;
	}

	.info-tag {
		font-size: 11px;
		color: var(--text-secondary);
		background: var(--bg-tertiary);
		border-radius: 3px;
		padding: 1px 5px;
	}

	.info-empty {
		font-size: 12px;
		color: var(--text-tertiary);
		line-height: 1.5;
		padding: 4px 0;
	}

	.info-versions-list {
		display: flex;
		flex-direction: column;
		gap: 1px;
		margin-top: 4px;
	}

	.info-version-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 8px;
		border-radius: 5px;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
		width: 100%;
	}

	.info-version-item:hover {
		background: var(--bg-hover);
	}

	.info-version-item.active {
		background: var(--bg-active);
	}

	.info-version-date {
		font-size: 12px;
		color: var(--text-primary);
	}

	.info-version-size {
		font-size: 11px;
		color: var(--text-tertiary);
	}

	.editor-container.mobile .editor-body-row:has(.info-panel) > .editor-body {
		display: none;
	}

	.editor-container.mobile .info-panel {
		width: 100%;
		border-left: none;
		border-top: 1px solid var(--border-light);
	}
</style>
