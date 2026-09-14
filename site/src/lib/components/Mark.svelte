<script lang="ts">
	import { markPath } from '$lib/mark';

	let { size = 28, title = '' }: { size?: number; title?: string } = $props();
</script>

<svg
	viewBox="0 0 1000 620"
	width={size}
	height={(size * 62) / 100}
	role={title ? 'img' : undefined}
	aria-hidden={title ? undefined : 'true'}
>
	{#if title}<title>{title}</title>{/if}
	<path d={markPath()} fill="currentColor" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
