<script lang="ts">
	let { onResize }: { onResize: (delta: number) => void } = $props();
	let active = $state(false);
	let startX = 0;
	let rafId = 0;
	let pendingDelta = 0;

	function onMouseDown(e: MouseEvent) {
		e.preventDefault();
		active = true;
		startX = e.clientX;
		document.body.classList.add('resizing');
		window.addEventListener('mousemove', onMouseMove);
		window.addEventListener('mouseup', onMouseUp);
	}

	function onMouseMove(e: MouseEvent) {
		pendingDelta += e.clientX - startX;
		startX = e.clientX;
		if (!rafId) {
			rafId = requestAnimationFrame(() => {
				onResize(pendingDelta);
				pendingDelta = 0;
				rafId = 0;
			});
		}
	}

	function onMouseUp() {
		active = false;
		document.body.classList.remove('resizing');
		if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
		if (pendingDelta) { onResize(pendingDelta); pendingDelta = 0; }
		window.removeEventListener('mousemove', onMouseMove);
		window.removeEventListener('mouseup', onMouseUp);
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="resize-handle" class:active onmousedown={onMouseDown}></div>
