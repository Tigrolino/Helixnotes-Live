<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getTasks } from '$lib/api';
	import { debounce } from '$lib/utils/debounce';
	import { tasksLayout, tasksHideCompleted, tasksOnlyFlagged, tasksSort, appConfig } from '$lib/stores/app';
	import { isAndroid } from '$lib/platform';
	import type { TaskItem, FileEvent } from '$lib/types';

	let { onOpenTask = (_t: TaskItem) => {}, onToggleTask = async (_t: TaskItem) => {}, onSetTaskPriority = async (_t: TaskItem, _p: string | null) => {}, onSetTaskDue = async (_t: TaskItem, _d: string | null) => {} }: {
		onOpenTask?: (t: TaskItem) => void;
		onToggleTask?: (t: TaskItem) => Promise<void>;
		onSetTaskPriority?: (t: TaskItem, p: string | null) => Promise<void>;
		onSetTaskDue?: (t: TaskItem, d: string | null) => Promise<void>;
	} = $props();

	let tasks = $state<TaskItem[]>([]);
	let loading = $state(true);
	let taskQuery = $state('');
	let unlisten: (() => void) | null = null;

	async function load() {
		try {
			tasks = await getTasks();
		} catch (e) {
			console.error('Failed to load tasks:', e);
			tasks = [];
		}
		loading = false;
	}

	const debouncedLoad = debounce(load, 400);

	onMount(() => {
		load();
		listen<FileEvent>('file-changed', () => debouncedLoad()).then((u) => (unlisten = u));
		return () => { unlisten?.(); };
	});

	// Local YYYY-MM-DD (en-CA gives ISO-style date)
	const today = new Date().toLocaleDateString('en-CA');

	const prioRank: Record<string, number> = { high: 0, med: 1, low: 2 };
	function prank(p: string | null) { return p ? (prioRank[p] ?? 3) : 3; }
	// Free-text task filter: matches the task text or its note title (case-insensitive).
	function matchesQuery(t: TaskItem): boolean {
		const q = taskQuery.trim().toLowerCase();
		if (!q) return true;
		return t.text.toLowerCase().includes(q) || t.note_title.toLowerCase().includes(q);
	}

	const filtered = $derived.by(() => {
		let list = tasks.slice();
		if ($tasksHideCompleted) list = list.filter((t) => !t.completed);
		if ($tasksOnlyFlagged) list = list.filter((t) => !!t.due || !!t.priority);
		list = list.filter(matchesQuery);
		list.sort((a, b) => {
			if (a.completed !== b.completed) return a.completed ? 1 : -1; // done last
			if ($tasksSort === 'priority') {
				const d = prank(a.priority) - prank(b.priority);
				if (d) return d;
			} else if ($tasksSort === 'note') {
				const d = a.note_title.localeCompare(b.note_title);
				if (d) return d;
			} else {
				const ad = a.due ?? '9999-99-99';
				const bd = b.due ?? '9999-99-99';
				if (ad !== bd) return ad < bd ? -1 : 1;
			}
			return a.text.localeCompare(b.text);
		});
		return list;
	});

	const openCount = $derived(tasks.filter((t) => !t.completed).length);

	function dueClass(due: string | null): string {
		if (!due) return '';
		if (due < today) return 'overdue';
		if (due === today) return 'duetoday';
		return '';
	}

	async function toggle(t: TaskItem) {
		await onToggleTask(t);
		await load();
	}

	let prioMenuFor = $state<string | null>(null);
	let dueMenuFor = $state<string | null>(null);
	const rowKey = (t: TaskItem) => t.note_path + ':' + t.line + ':' + t.raw_line;

	async function choosePriority(t: TaskItem, p: string | null) {
		prioMenuFor = null;
		await onSetTaskPriority(t, p);
		await load();
	}

	async function chooseDue(t: TaskItem, d: string | null) {
		dueMenuFor = null;
		await onSetTaskDue(t, d);
		await load();
	}

	// ── Calendar ──
	// Layout persists per-vault (tasksLayout store -> VaultState). Android is always List.
	const viewLayout = $derived(isAndroid ? 'list' : $tasksLayout);
	const calNow = new Date();
	let calMonth = $state(calNow.getMonth());
	let calYear = $state(calNow.getFullYear());
	let selectedDate = $state(today);

	const calMonthNames = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
	// 0 = Sunday, 1 = Monday (default). Day names + grid offset follow the setting.
	const weekStartsOn = $derived($appConfig?.week_start === 'sunday' ? 0 : 1);
	const DOW = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'];
	const calDayNames = $derived([...DOW.slice(weekStartsOn), ...DOW.slice(0, weekStartsOn)]);

	// Calendar tasks share the list's hide-completed filter (no sort).
	const calBase = $derived(tasks.filter((t) => (!$tasksHideCompleted || !t.completed) && (!$tasksOnlyFlagged || !!t.due || !!t.priority) && matchesQuery(t)));

	const tasksByDate = $derived.by(() => {
		const m = new Map<string, TaskItem[]>();
		for (const t of calBase) {
			if (!t.due) continue;
			const arr = m.get(t.due);
			if (arr) arr.push(t);
			else m.set(t.due, [t]);
		}
		return m;
	});

	const undatedTasks = $derived(calBase.filter((t) => !t.due));

	function byPriority(a: TaskItem, b: TaskItem) {
		if (a.completed !== b.completed) return a.completed ? 1 : -1;
		const d = prank(a.priority) - prank(b.priority);
		return d || a.text.localeCompare(b.text);
	}

	const selectedDayTasks = $derived(
		(selectedDate ? (tasksByDate.get(selectedDate) ?? []) : []).slice().sort(byPriority)
	);

	function calDays() {
		const firstWeekday = (new Date(calYear, calMonth, 1).getDay() - weekStartsOn + 7) % 7;
		const lastDate = new Date(calYear, calMonth + 1, 0).getDate();
		const cells: { day: number; date: string; current: boolean }[] = [];
		for (let i = 0; i < firstWeekday; i++) cells.push({ day: 0, date: '', current: false });
		for (let d = 1; d <= lastDate; d++) {
			const date = `${calYear}-${String(calMonth + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
			cells.push({ day: d, date, current: date === today });
		}
		return cells;
	}

	function calPrev() {
		if (calMonth === 0) { calMonth = 11; calYear -= 1; } else calMonth -= 1;
	}
	function calNext() {
		if (calMonth === 11) { calMonth = 0; calYear += 1; } else calMonth += 1;
	}
	function calToday() {
		const n = new Date();
		calMonth = n.getMonth();
		calYear = n.getFullYear();
		selectedDate = today;
	}

	function agendaTitle(date: string): string {
		if (date === today) return 'Today';
		const [y, mo, d] = date.split('-').map(Number);
		return new Date(y, mo - 1, d).toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
	}

	// Drag a task (from the agenda or "No due date") onto a day to (re)schedule it.
	let draggedTask = $state<TaskItem | null>(null);
	let dragOverDate = $state<string | null>(null);

	async function dropOnDay(date: string) {
		const t = draggedTask;
		draggedTask = null;
		dragOverDate = null;
		if (!t || t.due === date) return;
		await onSetTaskDue(t, date);
		await load();
	}
</script>

<div class="tasks-view">
	{#if prioMenuFor || dueMenuFor}
		<!-- svelte-ignore a11y_consider_explicit_label -->
		<button class="prio-backdrop" onclick={() => { prioMenuFor = null; dueMenuFor = null; }} aria-label="Close menu"></button>
	{/if}
	<div class="tasks-controls">
		<input class="tasks-filter" type="text" placeholder="Filter tasks…" bind:value={taskQuery} onkeydown={(e) => { if (e.key === 'Escape') taskQuery = ''; }} aria-label="Filter tasks by text" />
		<button class="tasks-ctl" class:active={$tasksHideCompleted} onclick={() => ($tasksHideCompleted = !$tasksHideCompleted)} title="Hide completed tasks">Hide done</button>
		<button class="tasks-ctl" class:active={$tasksOnlyFlagged} onclick={() => ($tasksOnlyFlagged = !$tasksOnlyFlagged)} title="Only show tasks with a due date or priority">Flagged</button>
		{#if !isAndroid}
			<div class="tasks-layout">
				<button class="tasks-ctl" class:active={viewLayout === 'list'} onclick={() => ($tasksLayout = 'list')} title="List view">List</button>
				<button class="tasks-ctl" class:active={viewLayout === 'calendar'} onclick={() => ($tasksLayout = 'calendar')} title="Calendar view">Calendar</button>
			</div>
		{/if}
		{#if viewLayout === 'list'}
			<div class="tasks-sort">
				{#each [['due', 'Due'], ['priority', 'Priority'], ['note', 'Note']] as [v, label]}
					<button class="tasks-ctl" class:active={$tasksSort === v} onclick={() => ($tasksSort = v as 'due' | 'priority' | 'note')}>{label}</button>
				{/each}
			</div>
		{/if}
	</div>

	{#snippet taskRow(t: TaskItem, dragMode: boolean)}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="task-row"
			class:done={t.completed}
			class:draggable={dragMode}
			draggable={dragMode}
			ondragstart={(e) => { if (!dragMode) return; draggedTask = t; if (e.dataTransfer) { e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', rowKey(t)); } }}
			ondragend={() => { draggedTask = null; dragOverDate = null; }}
		>
			<button class="task-check" class:checked={t.completed} onclick={(e) => { e.stopPropagation(); toggle(t); }} title={t.completed ? 'Mark not done' : 'Mark done'} aria-label="toggle done"></button>
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<div class="task-main" onclick={() => onOpenTask(t)}>
				<div class="task-line">
					<span class="task-prio-wrap">
						<button
							class="task-prio prio-{t.priority ?? 'none'}"
							class:set={!!t.priority}
							title="Set priority"
							aria-label="Set priority"
							onclick={(e) => { e.stopPropagation(); dueMenuFor = null; prioMenuFor = prioMenuFor === rowKey(t) ? null : rowKey(t); }}
						>{t.priority ?? '!'}</button>
						{#if prioMenuFor === rowKey(t)}
							<div class="prio-menu" role="menu">
								<button class="prio-opt prio-high" onclick={(e) => { e.stopPropagation(); choosePriority(t, 'high'); }}>High</button>
								<button class="prio-opt prio-med" onclick={(e) => { e.stopPropagation(); choosePriority(t, 'med'); }}>Med</button>
								<button class="prio-opt prio-low" onclick={(e) => { e.stopPropagation(); choosePriority(t, 'low'); }}>Low</button>
								<button class="prio-opt prio-clear" onclick={(e) => { e.stopPropagation(); choosePriority(t, null); }}>None</button>
							</div>
						{/if}
					</span>
					<span class="task-label">{t.text}</span>
					<span class="task-due-wrap">
						<button
							class="task-due {dueClass(t.due)}"
							class:set={!!t.due}
							title="Set due date"
							aria-label="Set due date"
							onclick={(e) => { e.stopPropagation(); prioMenuFor = null; dueMenuFor = dueMenuFor === rowKey(t) ? null : rowKey(t); }}
						>{t.due ?? 'due'}</button>
						{#if dueMenuFor === rowKey(t)}
							<div class="due-menu">
								<input
									class="due-input"
									type="date"
									value={t.due ?? ''}
									onclick={(e) => e.stopPropagation()}
									onchange={(e) => { e.stopPropagation(); chooseDue(t, (e.target as HTMLInputElement).value || null); }}
								/>
								{#if t.due}<button class="prio-opt prio-clear" onclick={(e) => { e.stopPropagation(); chooseDue(t, null); }}>Clear</button>{/if}
							</div>
						{/if}
					</span>
				</div>
				<div class="task-note">{t.note_title}</div>
			</div>
		</div>
	{/snippet}

	{#if loading}
		<div class="tasks-empty">Loading...</div>
	{:else if viewLayout === 'calendar'}
		<div class="tasks-cal-pane">
			<div class="cal">
				<div class="cal-nav">
					<button class="cal-nav-btn" onclick={calPrev} title="Previous month" aria-label="Previous month">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
					</button>
					<button class="cal-title" onclick={calToday} title="Jump to today">{calMonthNames[calMonth]} {calYear}</button>
					<button class="cal-nav-btn" onclick={calNext} title="Next month" aria-label="Next month">
						<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
					</button>
				</div>
				<div class="cal-grid">
					{#each calDayNames as name}<div class="cal-head">{name}</div>{/each}
					{#each calDays() as cell}
						{#if cell.day === 0}
							<div class="cal-cell empty"></div>
						{:else}
							{@const cnt = (tasksByDate.get(cell.date) ?? []).length}
							<button
								class="cal-cell"
								class:today={cell.current}
								class:active={cell.date === selectedDate}
								class:has-tasks={cnt > 0}
								class:drop-over={dragOverDate === cell.date}
								onclick={() => (selectedDate = cell.date)}
								ondragover={(e) => { if (draggedTask) { e.preventDefault(); dragOverDate = cell.date; } }}
								ondragleave={() => { if (dragOverDate === cell.date) dragOverDate = null; }}
								ondrop={(e) => { e.preventDefault(); dropOnDay(cell.date); }}
							>
								<span class="cal-day">{cell.day}</span>
								{#if cnt > 0}<span class="cal-count" class:overdue={cell.date < today}>{cnt}</span>{/if}
							</button>
						{/if}
					{/each}
				</div>
			</div>
			<div class="cal-agenda">
				<div class="agenda-section">
					<div class="agenda-head" class:overdue={selectedDate < today}>{agendaTitle(selectedDate)}</div>
					{#if selectedDayTasks.length === 0}
						<div class="tasks-empty-sm">Nothing due.</div>
					{:else}
						{#each selectedDayTasks as t (rowKey(t))}{@render taskRow(t, true)}{/each}
					{/if}
				</div>
				{#if undatedTasks.length}
					<div class="agenda-section">
						<div class="agenda-head">No due date ({undatedTasks.length})</div>
						{#each undatedTasks as t (rowKey(t))}{@render taskRow(t, true)}{/each}
					</div>
				{/if}
			</div>
		</div>
	{:else if filtered.length === 0}
		<div class="tasks-empty">No tasks. Add <code>- [ ] something</code> to any note.{#if openCount === 0 && tasks.length}{' '}All done!{/if}</div>
	{:else}
		<div class="tasks-list">
			{#each filtered as t (rowKey(t))}{@render taskRow(t, false)}{/each}
		</div>
	{/if}
</div>

<style>
	.tasks-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}
	.tasks-controls {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
		border-bottom: 1px solid var(--border-color);
		flex-wrap: wrap;
		flex-shrink: 0;
	}
	.tasks-sort {
		display: flex;
		gap: 2px;
		margin-left: auto;
	}
	.tasks-layout {
		display: flex;
		gap: 2px;
	}
	.tasks-ctl {
		padding: 3px 8px;
		border: 1px solid var(--border-color);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-secondary);
		font-size: 12px;
		cursor: pointer;
	}
	.tasks-ctl.active {
		background: var(--accent);
		color: #fff;
		border-color: var(--accent);
	}
	.tasks-filter {
		width: 150px;
		padding: 3px 8px;
		border: 1px solid var(--border-color);
		border-radius: 5px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		font-size: 12px;
	}
	.tasks-filter::placeholder { color: var(--text-tertiary); }
	.tasks-filter:focus { outline: none; border-color: var(--accent); }
	.tasks-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}
	.tasks-empty {
		padding: 24px 16px;
		color: var(--text-tertiary);
		font-size: 13px;
		text-align: center;
	}
	.task-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 7px 12px;
		border-bottom: 1px solid var(--border-color);
	}
	.task-row:hover {
		background: var(--bg-hover);
	}
	.task-row.draggable { cursor: grab; }
	.task-row.draggable:active { cursor: grabbing; }
	.task-check {
		width: 16px;
		height: 16px;
		margin-top: 2px;
		border: 2px solid var(--border-color);
		border-radius: 4px;
		background: var(--bg-primary);
		cursor: pointer;
		flex-shrink: 0;
		position: relative;
	}
	.task-check:hover {
		border-color: var(--accent);
	}
	.task-check.checked {
		background: var(--accent);
		border-color: var(--accent);
	}
	.task-check.checked::after {
		content: '';
		position: absolute;
		left: 3px;
		top: 0;
		width: 5px;
		height: 9px;
		border: solid #fff;
		border-width: 0 2px 2px 0;
		transform: rotate(45deg);
	}
	.task-main {
		flex: 1;
		min-width: 0;
		cursor: pointer;
	}
	.task-line {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 5px;
		font-size: 13px;
		color: var(--text-primary);
	}
	.task-row.done .task-label {
		text-decoration: line-through;
		color: var(--text-tertiary);
	}
	.task-label {
		word-break: break-word;
	}
	.task-prio-wrap {
		position: relative;
		display: inline-flex;
		flex-shrink: 0;
	}
	.task-prio {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		padding: 1px 5px;
		border-radius: 4px;
		color: #fff;
		border: none;
		cursor: pointer;
		line-height: 1.5;
	}
	.task-prio.prio-none {
		background: transparent;
		color: var(--text-tertiary);
		border: 1px dashed var(--border-color);
		opacity: 0.4;
		transition: opacity 0.1s;
	}
	.task-row:hover .task-prio.prio-none { opacity: 1; }
	.prio-high { background: #ef4444; }
	.prio-med { background: #f59e0b; }
	.prio-low { background: #64748b; }
	.prio-menu {
		position: absolute;
		top: 100%;
		left: 0;
		margin-top: 3px;
		z-index: 51;
		display: flex;
		flex-direction: column;
		min-width: 84px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
		overflow: hidden;
		padding: 3px;
		gap: 2px;
	}
	.prio-opt {
		text-align: left;
		padding: 4px 8px;
		font-size: 12px;
		border: none;
		border-radius: 4px;
		background: transparent;
		color: var(--text-primary);
		cursor: pointer;
	}
	.prio-opt:hover { background: var(--bg-hover); }
	.prio-opt.prio-high { color: #ef4444; background: transparent; }
	.prio-opt.prio-med { color: #f59e0b; background: transparent; }
	.prio-opt.prio-low { color: #64748b; background: transparent; }
	.prio-opt.prio-high:hover,
	.prio-opt.prio-med:hover,
	.prio-opt.prio-low:hover { background: var(--bg-hover); }
	.prio-opt.prio-clear { color: var(--text-tertiary); }
	.prio-backdrop {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: transparent;
		border: none;
		padding: 0;
		cursor: default;
	}
	.task-due-wrap {
		position: relative;
		display: inline-flex;
		flex-shrink: 0;
	}
	.task-due {
		font-size: 11px;
		padding: 1px 6px;
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-secondary);
		border: 1px solid var(--border-color);
		cursor: pointer;
		font-family: inherit;
		line-height: 1.5;
	}
	.task-due:not(.set) {
		background: transparent;
		border-style: dashed;
		color: var(--text-tertiary);
		opacity: 0.4;
		transition: opacity 0.1s;
	}
	.task-row:hover .task-due:not(.set) { opacity: 1; }
	.task-due.overdue {
		color: #ef4444;
		border-color: #ef4444;
	}
	.task-due.duetoday {
		color: #f59e0b;
		border-color: #f59e0b;
	}
	.due-menu {
		position: absolute;
		top: 100%;
		left: 0;
		margin-top: 3px;
		z-index: 51;
		display: flex;
		flex-direction: column;
		gap: 4px;
		background: var(--bg-primary);
		border: 1px solid var(--border-color);
		border-radius: 6px;
		box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
		padding: 6px;
	}
	.due-input {
		font-family: inherit;
		font-size: 12px;
		padding: 3px 5px;
		border: 1px solid var(--border-color);
		border-radius: 4px;
		background: var(--bg-secondary);
		color: var(--text-primary);
		color-scheme: light dark;
	}
	.task-note {
		font-size: 11px;
		color: var(--text-tertiary);
		margin-top: 2px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Calendar */
	.tasks-cal-pane {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
	}
	.cal {
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-color);
		flex-shrink: 0;
	}
	.cal-nav {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}
	.cal-nav-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border: none;
		border-radius: 5px;
		background: transparent;
		color: var(--text-secondary);
		cursor: pointer;
	}
	.cal-nav-btn:hover { background: var(--bg-hover); }
	.cal-title {
		border: none;
		background: transparent;
		color: var(--text-primary);
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
		padding: 3px 8px;
		border-radius: 5px;
	}
	.cal-title:hover { background: var(--bg-hover); }
	.cal-grid {
		display: grid;
		grid-template-columns: repeat(7, 1fr);
		gap: 2px;
	}
	.cal-head {
		text-align: center;
		font-size: 10px;
		font-weight: 600;
		color: var(--text-tertiary);
		padding: 2px 0;
	}
	.cal-cell {
		position: relative;
		aspect-ratio: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1px;
		border: 1px solid transparent;
		border-radius: 6px;
		background: transparent;
		color: var(--text-primary);
		font-size: 12px;
		cursor: pointer;
	}
	.cal-cell.empty { cursor: default; }
	.cal-cell:not(.empty):hover { background: var(--bg-hover); }
	.cal-cell.today { border-color: var(--accent); color: var(--accent); font-weight: 700; }
	.cal-cell.active { background: var(--accent); color: #fff; }
	.cal-cell.drop-over {
		border: 1px dashed var(--accent);
		background: var(--bg-hover);
	}
	.cal-count {
		min-width: 15px;
		height: 15px;
		padding: 0 4px;
		border-radius: 8px;
		background: var(--accent);
		color: #fff;
		font-size: 9px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		line-height: 1;
	}
	.cal-count.overdue { background: #ef4444; }
	.cal-cell.active .cal-count { background: rgba(255, 255, 255, 0.3); }
	.cal-agenda {
		flex: 1;
	}
	.agenda-section {
		border-bottom: 1px solid var(--border-color);
		padding-bottom: 4px;
	}
	.agenda-head {
		padding: 8px 12px 4px;
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--text-tertiary);
	}
	.agenda-head.overdue { color: #ef4444; }
	.tasks-empty-sm {
		padding: 6px 12px 10px;
		font-size: 12px;
		color: var(--text-tertiary);
	}
</style>
