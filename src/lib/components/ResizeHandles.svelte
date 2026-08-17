<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { isMobile } from '$lib/platform';

	type Dir =
		| 'North' | 'South' | 'East' | 'West'
		| 'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';

	// Resolved only on the client (onMount), so SSR/prerender never calls Tauri APIs.
	let appWindow = $state<ReturnType<typeof getCurrentWindow> | null>(null);
	let maximized = $state(false);

	onMount(() => {
		if (isMobile) return;
		const win = getCurrentWindow();
		appWindow = win;
		const refresh = () => win.isMaximized().then((m) => (maximized = m)).catch(() => {});
		refresh();
		const unlisten = win.onResized(() => refresh());
		return () => { unlisten.then((fn) => fn()); };
	});

	function start(e: MouseEvent, dir: Dir) {
		if (e.button !== 0 || !appWindow) return;
		e.preventDefault();
		appWindow.startResizeDragging(dir);
	}
</script>

{#if appWindow && !maximized}
	<div class="resize-layer" aria-hidden="true">
		<div class="rz rz-n" role="presentation" onmousedown={(e) => start(e, 'North')}></div>
		<div class="rz rz-s" role="presentation" onmousedown={(e) => start(e, 'South')}></div>
		<div class="rz rz-w" role="presentation" onmousedown={(e) => start(e, 'West')}></div>
		<div class="rz rz-e" role="presentation" onmousedown={(e) => start(e, 'East')}></div>
		<div class="rz rz-nw" role="presentation" onmousedown={(e) => start(e, 'NorthWest')}></div>
		<div class="rz rz-ne" role="presentation" onmousedown={(e) => start(e, 'NorthEast')}></div>
		<div class="rz rz-sw" role="presentation" onmousedown={(e) => start(e, 'SouthWest')}></div>
		<div class="rz rz-se" role="presentation" onmousedown={(e) => start(e, 'SouthEast')}></div>
	</div>
{/if}

<style>
	/* Transparent overlay; only the strips capture events, the rest passes through. */
	.resize-layer {
		position: fixed;
		inset: 0;
		pointer-events: none;
		z-index: 99999;
	}

	.rz {
		position: fixed;
		pointer-events: auto;
	}

	/* Edges, inset by the corner size so corners win at the corners. */
	.rz-n { top: 0; left: 14px; right: 14px; height: 6px; cursor: ns-resize; }
	.rz-s { bottom: 0; left: 14px; right: 14px; height: 6px; cursor: ns-resize; }
	.rz-w { left: 0; top: 14px; bottom: 14px; width: 6px; cursor: ew-resize; }
	.rz-e { right: 0; top: 14px; bottom: 14px; width: 6px; cursor: ew-resize; }

	/* Corners: larger, reliable grab targets. */
	.rz-nw { top: 0; left: 0; width: 14px; height: 14px; cursor: nwse-resize; }
	.rz-ne { top: 0; right: 0; width: 14px; height: 14px; cursor: nesw-resize; }
	.rz-sw { bottom: 0; left: 0; width: 14px; height: 14px; cursor: nesw-resize; }
	.rz-se { bottom: 0; right: 0; width: 14px; height: 14px; cursor: nwse-resize; }
</style>
