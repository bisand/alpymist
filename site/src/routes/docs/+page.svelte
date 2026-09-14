<script lang="ts">
	let { data } = $props();
</script>

<svelte:head>
	<title>Documentation — Alpymist</title>
	<meta name="description" content="How Alpymist is built, and the decisions behind it." />
</svelte:head>

<article class="prose">
	<p class="eyebrow">Documentation</p>
	<h1>How Alpymist works, and why</h1>
	<p class="lede">
		Alpymist writes its reasoning down. Each foundational choice is an architecture decision record:
		what forced the decision, what was chosen, and what it costs. They are the best introduction to the
		project there is.
	</p>

	<h2>Start here</h2>
	<ul class="cards">
		<li>
			<a href="/download/">
				<strong>Install</strong>
				<span>Add the signed repository to Alpine, or build an ISO.</span>
			</a>
		</li>
		<li>
			<a href="/docs/building/">
				<strong>Building from source</strong>
				<span>The Rust workspace, the builder container, and booting an image in QEMU.</span>
			</a>
		</li>
	</ul>

	<h2>Decisions</h2>
	<ol class="adrs">
		{#each data.adrs as adr (adr.slug)}
			<li>
				<a href="/docs/adr/{adr.slug}/">
					<span class="meta">
						<span class="num">ADR {adr.number}</span>
						{#if adr.status}<span class="status">{adr.status}</span>{/if}
						{#if adr.date}<time datetime={adr.date}>{adr.date}</time>{/if}
					</span>
					<strong>{adr.title}</strong>
					<span class="summary">{adr.summary}</span>
				</a>
			</li>
		{/each}
	</ol>
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

	.cards,
	.adrs {
		list-style: none;
		padding: 0 !important;
		display: grid;
		gap: 0.75rem;
	}

	.cards {
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr));
	}

	li + li {
		margin-top: 0 !important;
	}

	a {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		padding: 1rem 1.2rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--surface);
		color: var(--ink);
		text-decoration: none;
	}

	a:hover {
		border-color: var(--accent-deep);
	}

	strong {
		font-weight: 600;
	}

	.summary,
	.cards span {
		color: var(--ink-dim);
		font-size: 0.93rem;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		font-family: var(--mono);
		font-size: 0.75rem;
		color: var(--ink-dim);
	}

	.num {
		color: var(--accent);
	}

	.status {
		color: var(--green);
	}
</style>
