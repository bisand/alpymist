<script lang="ts">
	import { page } from '$app/state';
	import Mark from '$lib/components/Mark.svelte';
</script>

<svelte:head>
	<title>{page.status === 404 ? 'Not found' : 'Error'} — Alpymist</title>
</svelte:head>

<div class="wrap lost">
	<div class="mark"><Mark size={96} /></div>
	<p class="eyebrow">{page.status}</p>
	<h1>{page.status === 404 ? 'Lost in the mist' : 'Something went wrong'}</h1>
	<p>
		{page.status === 404
			? 'There is nothing at this address. It may have moved when the documentation did.'
			: page.error?.message}
	</p>
	<p><a href="/">Back to the start</a> · <a href="/docs/">Documentation</a></p>
</div>

<style>
	.lost {
		padding-block: 6rem;
		text-align: center;
	}

	.mark {
		display: flex;
		justify-content: center;
		color: var(--sky-low);
		margin-bottom: 1.5rem;
	}

	h1 {
		font-size: clamp(1.8rem, 4vw, 2.5rem);
		margin: 0.5rem 0 1rem;
	}

	p:not(.eyebrow) {
		color: var(--ink-dim);
	}
</style>
