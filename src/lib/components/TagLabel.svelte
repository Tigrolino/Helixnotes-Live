<script lang="ts">
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { appConfig, tagStyles } from '$lib/stores/app';
	import NotebookGlyph from './NotebookGlyph.svelte';
	import { decodeBuiltinNotebookIcon } from '$lib/utils/notebook-icons';
	import { isValidTagColor, lookupTagStyle } from '$lib/utils/tag-styles';

	let {
		name,
		size = 14,
		tone = 'none',
		showName = true
	}: {
		name: string;
		size?: number;
		tone?: 'none' | 'accent' | 'muted';
		showName?: boolean;
	} = $props();

	const style = $derived(lookupTagStyle(name, $tagStyles));
	const color = $derived.by(() => {
		const value = style?.color;
		return isValidTagColor(value) ? value : null;
	});
	const builtinIcon = $derived(decodeBuiltinNotebookIcon(style?.icon));
	const imageSrc = $derived.by(() => {
		const icon = style?.icon;
		const vault = $appConfig?.active_vault;
		if (!icon || builtinIcon || icon.startsWith('builtin:') || !vault) return null;
		return convertFileSrc(`${vault}/${icon}`);
	});
</script>

<span
	class={[
		'tag-label',
		`tone-${tone}`,
		{ 'has-color': !!color, 'has-icon': !!(builtinIcon || imageSrc) }
	]}
	style:--tag-color={color}
	title={`#${name}`}
>
	{#if builtinIcon}
		<span class="tag-icon">
			<NotebookGlyph icon={builtinIcon} {size} />
		</span>
	{:else if imageSrc}
		<img class="tag-image" src={imageSrc} alt="" width={size} height={size} />
	{:else}
		<span class="tag-hash">#</span>
	{/if}
	{#if showName}<span class="tag-name">{name}</span>{/if}
</span>

<style>
	.tag-label {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		max-width: 100%;
		color: inherit;
		line-height: 1.2;
	}

	.tag-label.has-color {
		color: var(--tag-color);
	}

	.tag-icon,
	.tag-image,
	.tag-hash {
		flex-shrink: 0;
	}

	.tag-icon {
		display: grid;
		place-items: center;
		color: inherit;
	}

	.tag-image {
		border-radius: 3px;
		object-fit: cover;
	}

	.tag-hash {
		font-weight: 600;
		color: inherit;
		opacity: 0.72;
	}

	.tag-label.has-color .tag-hash,
	.tag-label.has-icon .tag-hash,
	.tone-accent .tag-hash,
	.tone-muted .tag-hash {
		opacity: 1;
	}

	.tag-name {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.tone-accent {
		gap: 3px;
		padding: 1px 5px;
		border-radius: 3px;
		background: var(--accent-light);
		color: var(--text-accent);
		font-size: 10px;
	}

	.tone-accent.has-color {
		color: var(--tag-color);
		background: color-mix(in srgb, var(--tag-color) 18%, transparent);
	}

	.tone-muted {
		gap: 3px;
		padding: 1px 7px;
		border-radius: 10px;
		background: var(--bg-tertiary);
		color: var(--text-tertiary);
		font-size: 11px;
		letter-spacing: 0.01em;
	}

	.tone-muted.has-color {
		color: var(--tag-color);
		background: color-mix(in srgb, var(--tag-color) 16%, var(--bg-tertiary));
	}

	.tone-accent .tag-hash,
	.tone-muted .tag-hash {
		color: inherit;
		font-weight: 600;
	}
</style>
