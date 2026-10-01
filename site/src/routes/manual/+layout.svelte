<script lang="ts">
	import { page } from '$app/state';
	import { grouped } from '$lib/manual';

	let { data, children } = $props();

	const groups = $derived(grouped(data.chapters));
</script>

<div class="wrap manual">
	<aside>
		<nav aria-label="Manual">
			<a class="top" href="/manual/" aria-current={page.url.pathname === '/manual/' ? 'page' : undefined}
				>The manual</a
			>
			{#each groups as g (g.group)}
				<p class="group">{g.group}</p>
				<ul>
					{#each g.chapters as chapter (chapter.slug)}
						{@const href = `/manual/${chapter.slug}/`}
						<li>
							<a {href} aria-current={page.url.pathname === href ? 'page' : undefined}>{chapter.title}</a>
						</li>
					{/each}
				</ul>
			{/each}
		</nav>
	</aside>
	<div class="content">
		{@render children()}
	</div>
</div>

<style>
	.manual {
		display: grid;
		grid-template-columns: 15rem minmax(0, 1fr);
		gap: 3.5rem;
		padding-block: 3rem 5rem;
	}

	aside {
		position: sticky;
		top: 1.5rem;
		align-self: start;
		max-height: calc(100vh - 3rem);
		overflow-y: auto;
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

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	a {
		display: block;
		padding: 0.3rem 0.75rem;
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

	.top {
		font-weight: 600;
		color: var(--ink);
	}

	@media (max-width: 52rem) {
		.manual {
			grid-template-columns: minmax(0, 1fr);
			gap: 2rem;
		}

		aside {
			position: static;
			max-height: none;
			order: 2;
			border-top: 1px solid var(--line);
			padding-top: 2rem;
		}
	}
</style>
