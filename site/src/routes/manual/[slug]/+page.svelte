<script lang="ts">
	import { REPO } from '$lib/site';

	let { data } = $props();
</script>

<svelte:head>
	<title>{data.chapter.title} — Alpymist manual</title>
	<meta name="description" content={data.chapter.summary} />
</svelte:head>

<article class="prose">
	<p class="eyebrow">{data.chapter.group}</p>
	<h1>{data.chapter.title}</h1>

	{#if data.chapter.sections.length > 2}
		<nav class="toc" aria-label="On this page">
			{#each data.chapter.sections as section (section.id)}
				<a href="#{section.id}">{section.title}</a>
			{/each}
		</nav>
	{/if}

	<!-- Rendered at build time from the repository's own docs/manual. -->
	{@html data.html}

	<p class="source">
		<a href="{REPO}/blob/main/docs/manual/{data.chapter.file}.md">Edit this chapter on GitHub</a>
	</p>

	<nav class="pager" aria-label="Other chapters">
		{#if data.previous}
			<a href="/manual/{data.previous.slug}/"><span>← Previous</span>{data.previous.title}</a>
		{:else}<span></span>{/if}
		{#if data.next}
			<a class="next" href="/manual/{data.next.slug}/"><span>Next →</span>{data.next.title}</a>
		{/if}
	</nav>
</article>

<style>
	h1 {
		font-size: clamp(1.9rem, 4vw, 2.6rem);
		margin: 0.5rem 0 1.25rem;
	}

	.toc {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem 0.5rem;
		padding-bottom: 1.5rem;
		border-bottom: 1px solid var(--line);
		margin-bottom: 1.5rem;
	}

	.toc a {
		font-size: 0.85rem;
		padding: 0.15rem 0.6rem;
		border: 1px solid var(--line);
		border-radius: 999px;
		color: var(--ink-dim);
		text-decoration: none;
	}

	.toc a:hover {
		color: var(--ink);
		border-color: var(--accent-deep);
	}

	.source {
		margin-top: 3.5rem;
		font-size: 0.85rem;
	}

	.pager {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
		gap: 0.75rem;
		margin-top: 1.5rem;
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
