<script lang="ts">
	import { grouped } from '$lib/manual';

	let { data } = $props();

	const groups = $derived(grouped(data.chapters));
</script>

<svelte:head>
	<title>The manual — Alpymist</title>
	<meta
		name="description"
		content="How to install Alpymist, find your way around it, and change everything that can be changed."
	/>
</svelte:head>

<article class="prose">
	<p class="eyebrow">Manual</p>
	<h1>The Alpymist manual</h1>
	<p class="lede">
		How to install Alpymist, find your way around it, and change everything that can be changed.
		Start at the top if the desktop is new to you, or go straight to the chapter you need.
	</p>

	{#each groups as g (g.group)}
		<h2>{g.group}</h2>
		<ul class="cards">
			{#each g.chapters as chapter (chapter.slug)}
				<li>
					<a href="/manual/{chapter.slug}/">
						<strong>{chapter.title}</strong>
						<span>{chapter.summary}</span>
					</a>
				</li>
			{/each}
		</ul>
	{/each}

	<p class="why">
		The manual says how. Why each part is the way it is, is in the
		<a href="/docs/">decision records</a>.
	</p>
</article>

<style>
	h1 {
		font-size: clamp(2rem, 4vw, 2.75rem);
		margin: 0.5rem 0 1rem;
	}

	.lede {
		font-size: 1.15rem;
		color: var(--ink-dim);
	}

	.cards {
		list-style: none;
		padding: 0 !important;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 19rem), 1fr));
		gap: 0.75rem;
	}

	li + li {
		margin-top: 0 !important;
	}

	.cards a {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		height: 100%;
		padding: 0.9rem 1.1rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--surface);
		color: var(--ink);
		text-decoration: none;
	}

	.cards a:hover {
		border-color: var(--accent-deep);
	}

	.cards span {
		color: var(--ink-dim);
		font-size: 0.9rem;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.why {
		margin-top: 3rem;
		color: var(--ink-dim);
	}
</style>
