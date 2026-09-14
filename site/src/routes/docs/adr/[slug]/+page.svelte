<script lang="ts">
	import { REPO } from '$lib/site';

	let { data } = $props();
</script>

<svelte:head>
	<title>ADR {data.adr.number}: {data.adr.title} — Alpymist</title>
	<meta name="description" content={data.adr.summary} />
</svelte:head>

<article class="prose">
	<p class="eyebrow">ADR {data.adr.number}</p>
	<h1>{data.adr.title}</h1>
	<p class="meta">
		{#if data.adr.status}<span class="status">{data.adr.status}</span>{/if}
		{#if data.adr.date}<time datetime={data.adr.date}>{data.adr.date}</time>{/if}
		<a href="{REPO}/blob/main/docs/adr/{data.adr.slug}.md">View source</a>
	</p>

	<!-- Rendered at build time from the repository's own docs/adr. -->
	{@html data.html}

	<nav class="pager" aria-label="Other decisions">
		{#if data.previous}
			<a href="/docs/adr/{data.previous.slug}/"><span>← Previous</span>{data.previous.title}</a>
		{:else}<span></span>{/if}
		{#if data.next}
			<a class="next" href="/docs/adr/{data.next.slug}/"><span>Next →</span>{data.next.title}</a>
		{/if}
	</nav>
</article>

<style>
	h1 {
		font-size: clamp(1.9rem, 4vw, 2.6rem);
		margin: 0.5rem 0 0.75rem;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem 1.25rem;
		font-family: var(--mono);
		font-size: 0.8rem;
		color: var(--ink-dim);
		padding-bottom: 1.5rem;
		border-bottom: 1px solid var(--line);
		margin-bottom: 2rem;
	}

	.status {
		color: var(--green);
	}

	.pager {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
		gap: 0.75rem;
		margin-top: 4rem;
	}

	.pager a {
		display: flex;
		flex-direction: column;
		padding: 0.9rem 1.1rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		color: var(--ink);
		text-decoration: none;
	}

	.pager a:hover {
		border-color: var(--accent-deep);
	}

	.pager span {
		font-size: 0.8rem;
		color: var(--accent);
	}

	.next {
		text-align: right;
	}
</style>
