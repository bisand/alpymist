<script lang="ts">
	import DocsNav from '$lib/components/DocsNav.svelte';

	let { data, children } = $props();
</script>

<div class="wrap docs">
	<aside>
		<DocsNav adrs={data.adrs} />
	</aside>
	<div class="content">
		{@render children()}
	</div>
</div>

<style>
	.docs {
		display: grid;
		grid-template-columns: 15rem minmax(0, 1fr);
		gap: 3.5rem;
		padding-block: 3rem 5rem;
	}

	aside {
		position: sticky;
		top: 1.5rem;
		align-self: start;
	}

	@media (max-width: 52rem) {
		.docs {
			grid-template-columns: minmax(0, 1fr);
			gap: 2rem;
		}

		aside {
			position: static;
			order: 2;
			border-top: 1px solid var(--line);
			padding-top: 2rem;
		}
	}
</style>
