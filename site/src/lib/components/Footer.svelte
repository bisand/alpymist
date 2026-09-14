<script lang="ts">
	import Mark from './Mark.svelte';
	import { PKGS, REPO } from '$lib/site';
</script>

<footer>
	<div class="wrap cols">
		<div class="about">
			<Mark size={26} />
			<p>
				An opinionated desktop on Alpine Linux. Dual-licensed under
				<a href="{REPO}/blob/main/LICENSE-MIT">MIT</a> or
				<a href="{REPO}/blob/main/LICENSE-APACHE">Apache-2.0</a>.
			</p>
		</div>
		<ul>
			<li><a href="/download/">Download</a></li>
			<li><a href="/docs/">Documentation</a></li>
			<li><a href={PKGS}>Package repository</a></li>
			<li><a href={REPO}>Source</a></li>
			<li><a href="{REPO}/blob/main/SECURITY.md">Security</a></li>
		</ul>
	</div>
</footer>

<style>
	footer {
		border-top: 1px solid var(--line);
		background: var(--ridge);
		color: var(--ink-dim);
		font-size: 0.9rem;
		padding-block: 2.5rem;
	}

	.cols {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 1.5rem 3rem;
	}

	.about {
		display: flex;
		gap: 0.9rem;
		align-items: flex-start;
		max-width: 26rem;
		color: var(--sky-low);
	}

	.about p {
		margin: 0;
		color: var(--ink-dim);
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem 1.5rem;
	}

	a {
		color: var(--ink-dim);
	}

	a:hover {
		color: var(--ink);
	}
</style>
