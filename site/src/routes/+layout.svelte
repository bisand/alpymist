<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import Header from '$lib/components/Header.svelte';
	import Footer from '$lib/components/Footer.svelte';

	let { children } = $props();
</script>

<svelte:head>
	<meta property="og:site_name" content="Alpymist" />
	<meta property="og:url" content="https://alpymist.org{page.url.pathname}" />
</svelte:head>

<a class="skip" href="#main">Skip to content</a>
<Header overlay={page.url.pathname === '/'} />
<main id="main">
	{@render children()}
</main>
<Footer />

<style>
	:global(body) {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
	}

	main {
		flex: 1;
	}

	.skip {
		position: absolute;
		left: 1rem;
		top: -3rem;
		z-index: 20;
		background: var(--accent);
		color: var(--sky-high);
		padding: 0.4rem 0.8rem;
		border-radius: 4px;
	}

	.skip:focus {
		top: 1rem;
	}
</style>
