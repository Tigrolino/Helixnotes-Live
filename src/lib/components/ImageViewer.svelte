<script lang="ts">
	import { onMount } from 'svelte';

	let { src, alt = '', onclose }: {
		src: string;
		alt?: string;
		onclose: () => void;
	} = $props();

	const MIN_SCALE = 1;
	const MAX_SCALE = 5;
	const DOUBLE_TAP_MS = 300;
	const TAP_MOVE_THRESHOLD = 6;

	type Point = { x: number; y: number };
	type ViewerGesture =
		| { kind: 'pan'; start: Point; pan: Point }
		| { kind: 'pinch'; distance: number; imagePoint: Point };

	let canvasElement = $state<HTMLDivElement | null>(null);
	let imageElement = $state<HTMLImageElement | null>(null);
	let closeButton = $state<HTMLButtonElement | null>(null);
	let scale = $state(1);
	let panX = $state(0);
	let panY = $state(0);
	let interacting = $state(false);
	let gesture: ViewerGesture | null = null;
	let touchMoved = false;
	let lastTapAt = 0;
	let zoomPercent = $derived(Math.round(scale * 100));

	function clamp(value: number, min: number, max: number): number {
		return Math.min(max, Math.max(min, value));
	}

	function midpoint(first: Touch, second: Touch): Point {
		return {
			x: (first.clientX + second.clientX) / 2,
			y: (first.clientY + second.clientY) / 2,
		};
	}

	function touchDistance(first: Touch, second: Touch): number {
		return Math.hypot(second.clientX - first.clientX, second.clientY - first.clientY);
	}

	function panLimits(nextScale: number): Point {
		const canvasWidth = canvasElement?.clientWidth ?? window.innerWidth;
		const canvasHeight = canvasElement?.clientHeight ?? window.innerHeight;
		const imageWidth = imageElement?.clientWidth ?? 0;
		const imageHeight = imageElement?.clientHeight ?? 0;
		return {
			x: Math.max(0, (imageWidth * nextScale - canvasWidth) / 2),
			y: Math.max(0, (imageHeight * nextScale - canvasHeight) / 2),
		};
	}

	function applyTransform(nextScale: number, nextPanX: number, nextPanY: number) {
		const clampedScale = clamp(nextScale, MIN_SCALE, MAX_SCALE);
		const limits = panLimits(clampedScale);
		scale = clampedScale;
		panX = clamp(nextPanX, -limits.x, limits.x);
		panY = clamp(nextPanY, -limits.y, limits.y);
	}

	function resetViewer() {
		interacting = false;
		gesture = null;
		applyTransform(MIN_SCALE, 0, 0);
	}

	function zoomAt(point: Point, nextScale: number) {
		const canvasRect = canvasElement?.getBoundingClientRect();
		const centerX = canvasRect ? canvasRect.left + canvasRect.width / 2 : window.innerWidth / 2;
		const centerY = canvasRect ? canvasRect.top + canvasRect.height / 2 : window.innerHeight / 2;
		const imageX = (point.x - centerX - panX) / scale;
		const imageY = (point.y - centerY - panY) / scale;
		applyTransform(
			nextScale,
			point.x - centerX - imageX * nextScale,
			point.y - centerY - imageY * nextScale,
		);
	}

	function beginPan(touch: Touch) {
		gesture = {
			kind: 'pan',
			start: { x: touch.clientX, y: touch.clientY },
			pan: { x: panX, y: panY },
		};
	}

	function beginPinch(first: Touch, second: Touch) {
		const point = midpoint(first, second);
		const canvasRect = canvasElement?.getBoundingClientRect();
		const centerX = canvasRect ? canvasRect.left + canvasRect.width / 2 : window.innerWidth / 2;
		const centerY = canvasRect ? canvasRect.top + canvasRect.height / 2 : window.innerHeight / 2;
		gesture = {
			kind: 'pinch',
			distance: Math.max(1, touchDistance(first, second)),
			imagePoint: {
				x: (point.x - centerX - panX) / scale,
				y: (point.y - centerY - panY) / scale,
			},
		};
	}

	function handleTouchStart(event: TouchEvent) {
		event.preventDefault();
		interacting = true;
		if (event.touches.length >= 2) {
			touchMoved = true;
			beginPinch(event.touches[0], event.touches[1]);
			return;
		}
		if (event.touches.length === 1) {
			touchMoved = false;
			beginPan(event.touches[0]);
		}
	}

	function handleTouchMove(event: TouchEvent) {
		event.preventDefault();
		if (event.touches.length >= 2) {
			if (gesture?.kind !== 'pinch') beginPinch(event.touches[0], event.touches[1]);
			if (gesture?.kind !== 'pinch') return;
			const point = midpoint(event.touches[0], event.touches[1]);
			const canvasRect = canvasElement?.getBoundingClientRect();
			const centerX = canvasRect ? canvasRect.left + canvasRect.width / 2 : window.innerWidth / 2;
			const centerY = canvasRect ? canvasRect.top + canvasRect.height / 2 : window.innerHeight / 2;
			const nextScale = clamp(
				scale * (touchDistance(event.touches[0], event.touches[1]) / gesture.distance),
				MIN_SCALE,
				MAX_SCALE,
			);
			gesture.distance = Math.max(1, touchDistance(event.touches[0], event.touches[1]));
			applyTransform(
				nextScale,
				point.x - centerX - gesture.imagePoint.x * nextScale,
				point.y - centerY - gesture.imagePoint.y * nextScale,
			);
			touchMoved = true;
			return;
		}
		if (event.touches.length === 1 && gesture?.kind === 'pan') {
			const dx = event.touches[0].clientX - gesture.start.x;
			const dy = event.touches[0].clientY - gesture.start.y;
			if (Math.hypot(dx, dy) > TAP_MOVE_THRESHOLD) touchMoved = true;
			applyTransform(scale, gesture.pan.x + dx, gesture.pan.y + dy);
		}
	}

	function handleTouchEnd(event: TouchEvent) {
		event.preventDefault();
		if (event.touches.length >= 2) {
			beginPinch(event.touches[0], event.touches[1]);
			return;
		}
		if (event.touches.length === 1) {
			beginPan(event.touches[0]);
			touchMoved = true;
			return;
		}

		interacting = false;
		gesture = null;
		if (!touchMoved && event.changedTouches.length > 0) {
			const now = performance.now();
			const touch = event.changedTouches[0];
			if (now - lastTapAt <= DOUBLE_TAP_MS) {
				if (scale > MIN_SCALE + 0.01) resetViewer();
				else zoomAt({ x: touch.clientX, y: touch.clientY }, 2);
				lastTapAt = 0;
			} else {
				lastTapAt = now;
			}
		}
	}

	function handleTouchCancel(event: TouchEvent) {
		event.preventDefault();
		interacting = false;
		gesture = null;
		touchMoved = false;
		applyTransform(scale, panX, panY);
	}

	function attachTouchGestures(node: HTMLElement) {
		node.addEventListener('touchstart', handleTouchStart, { passive: false });
		node.addEventListener('touchmove', handleTouchMove, { passive: false });
		node.addEventListener('touchend', handleTouchEnd, { passive: false });
		node.addEventListener('touchcancel', handleTouchCancel, { passive: false });
		return {
			destroy() {
				node.removeEventListener('touchstart', handleTouchStart);
				node.removeEventListener('touchmove', handleTouchMove);
				node.removeEventListener('touchend', handleTouchEnd);
				node.removeEventListener('touchcancel', handleTouchCancel);
			},
		};
	}

	onMount(() => {
		closeButton?.focus();
		const handleResize = () => applyTransform(scale, panX, panY);
		window.addEventListener('resize', handleResize);
		return () => window.removeEventListener('resize', handleResize);
	});
</script>

<div
	class="image-viewer"
	role="dialog"
	aria-modal="true"
	aria-label={alt ? `Image viewer: ${alt}` : 'Image viewer'}
	tabindex="-1"
	onkeydown={(event) => {
		if (event.key === 'Escape') {
			event.preventDefault();
			onclose();
		}
	}}
>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="image-viewer-canvas"
		bind:this={canvasElement}
		use:attachTouchGestures
		onclick={(event) => {
			if (event.target === event.currentTarget) onclose();
		}}
		onkeydown={(event) => {
			if (event.target === event.currentTarget && (event.key === 'Enter' || event.key === ' ')) onclose();
		}}
	>
		<img
			bind:this={imageElement}
			class:interacting
			src={src}
			alt={alt}
			draggable="false"
			style:transform={`translate3d(${panX}px, ${panY}px, 0) scale(${scale})`}
			onload={resetViewer}
		/>
	</div>

	<div class="image-viewer-controls">
		<button
			type="button"
			class="image-viewer-reset"
			onclick={resetViewer}
			disabled={scale === MIN_SCALE && panX === 0 && panY === 0}
			aria-label={`Reset zoom, currently ${zoomPercent}%`}
		>
			<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
				<path d="M3 12a9 9 0 1 0 3-6.7" />
				<path d="M3 4v6h6" />
			</svg>
			<span>{zoomPercent}%</span>
		</button>
		<button bind:this={closeButton} type="button" class="image-viewer-close" onclick={onclose} aria-label="Close image viewer">
			<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
				<path d="M18 6 6 18M6 6l12 12" />
			</svg>
		</button>
	</div>

	<div class="image-viewer-hint" aria-hidden="true">Pinch to zoom · drag to move · double-tap to zoom or reset</div>
	<span class="sr-only" aria-live="polite">Zoom {zoomPercent}%</span>
</div>

<style>
	.image-viewer {
		position: fixed;
		inset: 0;
		z-index: 10000;
		background: #06080c;
		color: #fff;
		overflow: hidden;
		overscroll-behavior: contain;
	}

	.image-viewer-canvas {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
	}

	.image-viewer-canvas img {
		display: block;
		max-width: calc(100vw - 24px);
		max-height: calc(100dvh - 24px);
		object-fit: contain;
		transform-origin: center center;
		transition: transform 160ms ease-out;
		will-change: transform;
		-webkit-user-drag: none;
		user-select: none;
	}

	.image-viewer-canvas img.interacting {
		transition: none;
	}

	.image-viewer-controls {
		position: absolute;
		inset: max(12px, env(safe-area-inset-top)) max(12px, env(safe-area-inset-right)) auto max(12px, env(safe-area-inset-left));
		display: flex;
		align-items: center;
		justify-content: space-between;
		pointer-events: none;
	}

	.image-viewer-controls button {
		min-height: 44px;
		border: 1px solid rgba(255, 255, 255, 0.18);
		background: rgba(20, 23, 30, 0.86);
		color: #fff;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
		pointer-events: auto;
	}

	.image-viewer-reset {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 14px;
		border-radius: 10px;
		font: inherit;
		font-size: 13px;
		font-weight: 600;
	}

	.image-viewer-reset:disabled {
		opacity: 0.45;
	}

	.image-viewer-close {
		display: grid;
		place-items: center;
		width: 44px;
		padding: 0;
		border-radius: 50%;
	}

	.image-viewer-hint {
		position: absolute;
		left: 50%;
		bottom: max(18px, calc(env(safe-area-inset-bottom) + 10px));
		transform: translateX(-50%);
		width: max-content;
		max-width: calc(100vw - 32px);
		padding: 8px 12px;
		border-radius: 8px;
		background: rgba(20, 23, 30, 0.82);
		color: rgba(255, 255, 255, 0.72);
		font-size: 12px;
		line-height: 1.35;
		text-align: center;
		pointer-events: none;
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}

	@media (prefers-reduced-motion: reduce) {
		.image-viewer-canvas img {
			transition: none;
		}
	}
</style>
