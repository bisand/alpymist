<script lang="ts">
	import { page } from '$app/state';

	type Entry = { slug: string; number: string; title: string };

	let { adrs }: { adrs: Entry[] } = $props();
</script>

<nav aria-label="Documentation">
	<p class="group">Guides</p>
	<ul>
		<li><a href="/docs/" aria-current={page.url.pathname === '/docs/' ? 'page' : undefined}>Overview</a></li>
		<li>
			<a href="/docs/building/" aria-current={page.url.pathname === '/docs/building/' ? 'page' : undefined}
				>Building from source</a
			>
		</li>
	</ul>
	<p class="group">Decisions</p>
	<ul>
		{#each adrs as adr (adr.slug)}
			{@const href = `/docs/adr/${adr.slug}/`}
			<li>
				<a {href} aria-current={page.url.pathname === href ? 'page' : undefined}>
					<span class="num">{adr.number}</span>
					{adr.title}
				</a>
			</li>
		{/each}
	</ul>
</nav>

<style>
	nav {
		font-size: 0.92rem;
	}

	.group {
		font-family: var(--mono);
		font-size: 0.72rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--sky-low);
		margin: 1.5rem 0 0.5rem;
	}

	.group:first-child {
		margin-top: 0;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	a {
		display: block;
		padding: 0.35rem 0.75rem;
		border-left: 2px solid var(--line);
		color: var(--ink-dim);
		text-decoration: none;
		line-height: 1.4;
	}

	a:hover {
		color: var(--ink);
	}

	a[aria-current='page'] {
		color: var(--ink);
		border-left-color: var(--accent);
	}

	.num {
		font-family: var(--mono);
		font-size: 0.8em;
		color: var(--accent);
		margin-right: 0.3rem;
	}
</style>
