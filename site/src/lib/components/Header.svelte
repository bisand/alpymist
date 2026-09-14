<script lang="ts">
	import { page } from '$app/state';
	import Mark from './Mark.svelte';
	import { REPO, nav } from '$lib/site';

	let { overlay = false }: { overlay?: boolean } = $props();

	const current = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
</script>

<header class:overlay>
	<div class="wrap bar">
		<a class="brand" href="/" aria-label="Alpymist home">
			<Mark size={30} />
			<span>alpymist</span>
		</a>
		<nav aria-label="Main">
			{#each nav.slice(1) as item (item.href)}
				<a href={item.href} aria-current={current(item.href) ? 'page' : undefined}>{item.label}</a>
			{/each}
			<a href={REPO} class="gh">GitHub</a>
		</nav>
	</div>
</header>

<style>
	header {
		position: relative;
		z-index: 10;
		border-bottom: 1px solid var(--line);
		background: color-mix(in srgb, var(--sky-high) 88%, transparent);
		backdrop-filter: blur(8px);
	}

	header.overlay {
		position: absolute;
		inset: 0 0 auto;
		border-bottom-color: transparent;
		background: transparent;
		backdrop-filter: none;
	}

	.bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		min-height: 4rem;
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: var(--accent);
		text-decoration: none;
	}

	.brand span {
		font-family: var(--mono);
		font-weight: 500;
		font-size: 1.05rem;
		color: var(--ink);
		letter-spacing: 0.02em;
	}

	nav {
		display: flex;
		align-items: center;
		gap: clamp(0.75rem, 3vw, 1.75rem);
	}

	nav a {
		color: var(--ink-dim);
		text-decoration: none;
		font-size: 0.95rem;
		font-weight: 500;
		padding-block: 0.25rem;
		border-bottom: 2px solid transparent;
	}

	nav a:hover {
		color: var(--ink);
	}

	nav a[aria-current='page'] {
		color: var(--ink);
		border-bottom-color: var(--accent);
	}

	@media (max-width: 30rem) {
		.brand span {
			display: none;
		}
	}
</style>
