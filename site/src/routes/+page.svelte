<script lang="ts">
	import Backdrop from '$lib/components/Backdrop.svelte';
	import { REPO, tiers } from '$lib/site';

	const pillars = [
		{
			tag: 'scales down',
			title: 'One desktop, three renderers',
			body: 'The same keybindings, theme, panel and launcher, drawn by Hyprland on a good GPU, by labwc on a weak one or on the CPU alone, and by i3 where Wayland cannot run. The installer probes the machine and says why it chose what it chose.',
			href: '/docs/adr/0001-hardware-tiers/'
		},
		{
			tag: 'signed packages',
			title: 'No curl | sh. Ever.',
			body: 'Every piece of Alpymist, down to the wallpaper, is an apk in a repository whose index is signed with a key CI never sees. You boot a signed ISO, or you apk add it onto Alpine.',
			href: '/docs/adr/0002-supply-chain/'
		},
		{
			tag: 'rust',
			title: 'First-party code in Rust',
			body: 'The installer, splash, menu and tier probe are Rust. unsafe is denied workspace-wide and confined to one short file that talks to EGL, where there is no other way.',
			href: '/docs/adr/0004-unsafe-and-musl-linking/'
		},
		{
			tag: 'musl + flatpak',
			title: 'A base you can audit',
			body: 'Pure musl Alpine underneath. Proprietary and glibc-only software runs through Flatpak, in a sandbox, rather than natively beside it.',
			href: '/docs/adr/0003-foundational-choices/'
		}
	];
</script>

<svelte:head>
	<title>Alpymist — an Alpine desktop that scales down</title>
	<meta
		name="description"
		content="An opinionated desktop on Alpine Linux: Hyprland where the hardware allows, software-rendered Wayland where it does not, shipped only as signed packages."
	/>
</svelte:head>

<section class="hero">
	<Backdrop />
	<div class="wrap hero-inner">
		<p class="eyebrow">Alpine + mist · early days</p>
		<h1>A curated desktop for the machine you already have.</h1>
		<p class="lede">
			Alpymist is an opinionated Wayland desktop on Alpine Linux. It runs Hyprland where the
			hardware can, composites on the CPU where it cannot, and ships as nothing but signed
			packages.
		</p>
		<div class="actions">
			<a class="button primary" href="/download/">Get Alpymist</a>
			<a class="button" href="/docs/">Read the docs</a>
		</div>
	</div>
</section>

<section class="wrap name">
	<p>
		The name is <em>Alpine</em> plus <em>mist</em>, the haze on the mountains. It is also a pun on
		<em>alchemist</em>, which is roughly what turning a fifteen-year-old laptop back into a usable
		desktop amounts to.
	</p>
</section>

<section class="wrap pillars" aria-label="What makes it different">
	{#each pillars as p (p.title)}
		<a class="pillar" href={p.href}>
			<span class="tag">{p.tag}</span>
			<h2>{p.title}</h2>
			<p>{p.body}</p>
			<span class="more">Read the decision →</span>
		</a>
	{/each}
</section>

<section class="wrap tiers">
	<div class="tiers-head">
		<p class="eyebrow">Hardware tiers</p>
		<h2>Picked by probing, never by guessing</h2>
		<p>
			At install and again on first boot, Alpymist checks what the GPU, driver and memory can
			actually do and picks a tier. Anything it cannot confirm counts against the machine: a
			desktop that starts slowly can be fixed, one that does not start cannot.
		</p>
	</div>
	<ol class="tier-list">
		{#each tiers as tier, i (tier.name)}
			<li style="--depth: {i}">
				<div class="tier-name">
					<code>{tier.name}</code>
					<span>{tier.backend}</span>
				</div>
				<p class="when">{tier.when}</p>
				<p class="note">{tier.note}</p>
			</li>
		{/each}
	</ol>
</section>

<section class="wrap status">
	<div class="status-card">
		<div>
			<p class="eyebrow">Status</p>
			<h2>Early, and honest about it</h2>
			<p>
				The build pipeline runs end to end: CI builds a bootable ISO, boots it in QEMU and checks
				the tier it reports. Packages are published to a signed repository. It is not yet something
				to install on a machine you depend on.
			</p>
		</div>
		<div class="actions">
			<a class="button primary" href="/download/">Download options</a>
			<a class="button" href={REPO}>Follow on GitHub</a>
		</div>
	</div>
</section>

<style>
	.hero {
		position: relative;
		min-height: min(92vh, 52rem);
		display: flex;
		align-items: flex-start;
		overflow: hidden;
		isolation: isolate;
	}

	.hero :global(.backdrop) {
		z-index: -1;
	}

	.hero-inner {
		padding-block: clamp(7rem, 18vh, 11rem) 4rem;
	}

	h1 {
		font-size: clamp(2.2rem, 5.5vw, 4rem);
		font-weight: 600;
		max-width: 16ch;
		margin: 0.75rem 0 1.25rem;
		letter-spacing: -0.025em;
	}

	.lede {
		font-size: clamp(1.05rem, 1.8vw, 1.25rem);
		color: var(--mist);
		max-width: 36rem;
		margin: 0 0 2rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
	}

	.button {
		display: inline-flex;
		align-items: center;
		padding: 0.65rem 1.2rem;
		border-radius: 6px;
		border: 1px solid var(--line-strong);
		background: color-mix(in srgb, var(--sky-high) 70%, transparent);
		color: var(--ink);
		font-weight: 500;
		text-decoration: none;
	}

	.button:hover {
		border-color: var(--accent);
	}

	.button.primary {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--sky-high);
		font-weight: 600;
	}

	.button.primary:hover {
		background: var(--ink);
		border-color: var(--ink);
	}

	.name {
		padding-block: 4rem 1rem;
	}

	.name p {
		max-width: 46rem;
		font-size: clamp(1.1rem, 2vw, 1.35rem);
		color: var(--ink-dim);
		margin: 0;
	}

	.name em {
		font-style: normal;
		color: var(--ink);
	}

	.pillars {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 16rem), 1fr));
		gap: 1rem;
		padding-block: 3rem 5rem;
	}

	.pillar {
		display: flex;
		flex-direction: column;
		padding: 1.5rem;
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--surface);
		color: inherit;
		text-decoration: none;
		transition: border-color 0.15s;
	}

	.pillar:hover {
		border-color: var(--accent-deep);
	}

	.tag {
		font-family: var(--mono);
		font-size: 0.75rem;
		color: var(--accent);
	}

	.pillar h2 {
		font-size: 1.2rem;
		margin: 0.5rem 0;
	}

	.pillar p {
		color: var(--ink-dim);
		font-size: 0.95rem;
		margin: 0 0 1rem;
		flex: 1;
	}

	.more {
		font-size: 0.85rem;
		color: var(--accent);
	}

	.tiers {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 22rem), 1fr));
		gap: 2rem 4rem;
		padding-block: 3rem 5rem;
		border-top: 1px solid var(--line);
	}

	.tiers-head h2,
	.status h2 {
		font-size: clamp(1.6rem, 3vw, 2.2rem);
		margin: 0.5rem 0 1rem;
	}

	.tiers-head p:not(.eyebrow) {
		color: var(--ink-dim);
	}

	.tier-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.75rem;
	}

	.tier-list li {
		/* Each tier down is further off, hazed like a distant ridge. */
		padding: 1rem 1.25rem;
		border-radius: var(--radius);
		border-left: 3px solid color-mix(in srgb, var(--accent) calc(100% - var(--depth) * 22%), var(--sky-low));
		background: color-mix(in srgb, var(--surface) calc(100% - var(--depth) * 12%), var(--sky-high));
	}

	.tier-name {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0.25rem 0.75rem;
	}

	.tier-name code {
		background: none;
		border: 0;
		padding: 0;
		color: var(--ink);
		font-size: 1rem;
		font-weight: 500;
	}

	.tier-name span {
		color: var(--accent);
		font-size: 0.9rem;
	}

	.when,
	.note {
		margin: 0.2rem 0 0;
		font-size: 0.9rem;
	}

	.when {
		color: var(--mist);
	}

	.note {
		color: var(--ink-dim);
	}

	.status {
		padding-block: 0 5rem;
	}

	.status-card {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		justify-content: space-between;
		gap: 1.5rem 3rem;
		padding: clamp(1.5rem, 4vw, 2.5rem);
		border: 1px solid var(--line);
		border-radius: 12px;
		background: linear-gradient(135deg, var(--surface), var(--ridge));
	}

	.status-card > div:first-child {
		max-width: 38rem;
	}

	.status-card p:not(.eyebrow) {
		color: var(--ink-dim);
		margin: 0;
	}
</style>
