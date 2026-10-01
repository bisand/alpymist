<script lang="ts">
	import Backdrop from '$lib/components/Backdrop.svelte';
	import { RELEASES, REPO, features, verdicts } from '$lib/site';
	import menu from '$lib/assets/shots/menu.webp';
	import terminals from '$lib/assets/shots/terminals.webp';
	import vscode from '$lib/assets/shots/vscode.webp';
	import libreoffice from '$lib/assets/shots/libreoffice.webp';
	import appearance from '$lib/assets/shots/settings-appearance.webp';
	import displays from '$lib/assets/shots/settings-displays.webp';
	import saver from '$lib/assets/shots/saver-mountains.webp';
	import lock from '$lib/assets/shots/lock.webp';

	// Captured with grim from a ThinkPad X1 Carbon on the dev channel, except
	// the lock screen: a session lock shows nothing else while it is up, so it
	// cannot be captured from inside the session. That one is drawn by the lock
	// screen's own code (crates/alpymist-lock/examples/snapshot.rs).
	const leads = [
		{
			src: menu,
			alt: 'The Alpymist desktop: a bar along the top, a terminal showing the probe\'s report, and the menu open over a mountain wallpaper.',
			caption: 'The menu on Super+Space, over a terminal showing what the probe says of this machine.'
		},
		{
			src: terminals,
			alt: 'Three tiled terminals: a shell that has run alpymist probe, nano editing the Hyprland configuration, and btop showing processor, memory, disks and processes.',
			caption: 'Three terminals, tiled: a shell, nano on the Hyprland configuration, and btop.'
		}
	];
	const shots = [
		{
			src: appearance,
			alt: 'The Settings window on its Appearance page, with style, accent colour, text size and a grid of wallpapers.',
			caption: 'Settings › Appearance'
		},
		{
			src: displays,
			alt: 'The Settings window on its Displays page, with three screens side by side and the built-in one selected.',
			caption: 'Settings › Displays, with a dock\'s two screens'
		},
		{
			src: vscode,
			alt: 'Visual Studio Code filling the screen, with the Alpymist source tree on the left and a Rust file open.',
			caption: 'Visual Studio Code, from Flathub'
		},
		{
			src: libreoffice,
			alt: 'LibreOffice Writer with a one-page document about Alpymist: headings, paragraphs and a small table.',
			caption: 'LibreOffice Writer'
		},
		{
			src: saver,
			alt: 'A screensaver of dark, pixelated mountain ranges in mist under a few stars.',
			caption: 'The Mountains screensaver'
		},
		{
			src: lock,
			alt: 'The lock screen: a clock and date over a photograph of the Milky Way above mountains, and a card with the account\'s name and a password field.',
			caption: 'The lock screen, which is the login screen'
		}
	];

	const pillars = [
		{
			tag: 'one desktop',
			title: 'Hyprland, and only Hyprland',
			body: 'One desktop, configured once and kept in focus: Hyprland on Wayland, with its keybindings, theme, bar and launcher. The installer probes the machine and says how well it will run there, and why.',
			href: '/docs/adr/0001-hardware-tiers/'
		},
		{
			tag: 'signed packages',
			title: 'No curl | sh. Ever.',
			body: 'Every piece of Alpymist, down to the wallpaper, is an apk in a signed repository, built in the open from a tagged commit. You boot the installer, or you apk add it onto Alpine, and upgrades are apk upgrade.',
			href: '/docs/adr/0002-supply-chain/'
		},
		{
			tag: 'secure by default',
			title: 'Nothing unsafe is on',
			body: 'No automatic login, no passwordless root, no listening service, no secret kept in the clear. The means to weaken any of it ship as a switch that says what it gives up, and turns back off.',
			href: '/docs/adr/0011-secure-by-default/'
		},
		{
			tag: 'rust',
			title: 'First-party code in Rust',
			body: 'The installer, the login and lock screens, Settings, the menu and the popups under the bar are Rust. unsafe is denied workspace-wide and confined to one short file that talks to EGL, where there is no other way.',
			href: '/docs/adr/0004-unsafe-and-musl-linking/'
		},
		{
			tag: 'musl + flatpak',
			title: 'A base you can audit',
			body: 'Pure musl Alpine underneath. Proprietary and glibc-only software runs through Flatpak, in a sandbox, rather than natively beside it.',
			href: '/docs/adr/0003-foundational-choices/'
		},
		{
			tag: 'written down',
			title: 'Every decision has a record',
			body: 'What forced each choice, what was chosen and what it costs. A decision that is reversed gets a dated addendum, and the original stays.',
			href: '/docs/'
		}
	];
</script>

<svelte:head>
	<title>Alpymist — a Hyprland desktop on Alpine</title>
	<meta
		name="description"
		content="An opinionated desktop on Alpine Linux: Hyprland on Wayland, configured once and kept in focus, secure by default, shipped only as signed packages."
	/>
</svelte:head>

<section class="hero">
	<Backdrop picture />
	<div class="wrap hero-inner">
		<p class="eyebrow">Alpine + mist · early days</p>
		<h1>A curated desktop for the machine you already have.</h1>
		<p class="lede">
			Alpymist is an opinionated Wayland desktop on Alpine Linux: Hyprland, configured once and
			kept in focus, secure by default, and shipped as nothing but signed packages.
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

<section class="wrap desktop">
	<div class="desktop-head">
		<p class="eyebrow">The desktop</p>
		<h2>What is in it</h2>
		<p>
			A login screen, a bar, a menu on Super+Space and a Settings app, with a terminal, a browser
			and Flatpak behind them. These are the parts Alpymist wrote itself, and each has a record
			of why it is the way it is.
		</p>
	</div>
	<div class="leads">
		{#each leads as shot (shot.src)}
			<figure>
				<a href={shot.src}
					><img src={shot.src} alt={shot.alt} width="1600" height="900" loading="lazy" /></a
				>
				<figcaption>{shot.caption}</figcaption>
			</figure>
		{/each}
	</div>
	<div class="shots">
		{#each shots as shot (shot.src)}
			<figure>
				<a href={shot.src}
					><img src={shot.src} alt={shot.alt} width="1600" height="900" loading="lazy" /></a
				>
				<figcaption>{shot.caption}</figcaption>
			</figure>
		{/each}
	</div>
	<p class="ahead">
		Taken on a ThinkPad X1 Carbon following the dev channel, on 1 October 2026. The lock screen is
		drawn by its own code rather than captured: while it is up, the compositor shows nothing else.
	</p>
	<ul class="feature-list">
		{#each features as f (f.adr)}
			<li>
				<a href="/docs/adr/{f.adr}/">
					<h3>{f.title}</h3>
					<p>{f.body}</p>
				</a>
			</li>
		{/each}
	</ul>
	<p class="ahead">
		The newest of these reach the dev channel first, and the installer image with the release
		after. <a href="/download/#channels">How the channels work</a>.
	</p>
</section>

<section class="wrap tiers">
	<div class="tiers-head">
		<p class="eyebrow">Hardware</p>
		<h2>Said by probing, never by guessing</h2>
		<p>
			At install and again on first boot, Alpymist checks what the GPU, driver and memory can
			actually do, and says how Hyprland will run. Anything it cannot confirm counts against the
			machine, and nothing is refused on it: you decide.
		</p>
	</div>
	<ol class="tier-list">
		{#each verdicts as tier, i (tier.name)}
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
			<h2>Early, and installable</h2>
			<p>
				Each release comes with an installer image for x86_64 and for aarch64, built by CI from
				the same packages the signed repository serves; the x86_64 one is booted in QEMU before
				those packages are published. Installed systems upgrade with <code>apk upgrade</code>. It is
				young software with rough edges: try it in a virtual machine, or on hardware you do not
				need tomorrow.
			</p>
		</div>
		<div class="actions">
			<a class="button primary" href="/download/">Download options</a>
			<a class="button" href={RELEASES}>Latest release</a>
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
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 19rem), 1fr));
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

	.desktop {
		padding-block: 3rem 4rem;
		border-top: 1px solid var(--line);
	}

	.desktop-head {
		max-width: 46rem;
	}

	.desktop-head p:not(.eyebrow) {
		color: var(--ink-dim);
	}

	figure {
		margin: 0;
	}

	figure img {
		display: block;
		width: 100%;
		height: auto;
		border: 1px solid var(--line-strong);
		border-radius: var(--radius);
		background: var(--surface);
	}

	figure a:hover img {
		border-color: var(--accent-deep);
	}

	figcaption {
		margin-top: 0.5rem;
		font-size: 0.88rem;
		color: var(--ink-dim);
	}

	.leads {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 26rem), 1fr));
		gap: 1.25rem;
		margin-top: 2rem;
	}

	.shots {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 19rem), 1fr));
		gap: 1.25rem;
		margin-top: 1.25rem;
	}

	.feature-list {
		list-style: none;
		margin: 2rem 0 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 20rem), 1fr));
		gap: 0 3rem;
	}

	.feature-list a {
		display: block;
		height: 100%;
		padding: 1.1rem 0;
		border-top: 1px solid var(--line);
		color: inherit;
		text-decoration: none;
	}

	.feature-list h3 {
		font-size: 1.05rem;
		margin: 0 0 0.3rem;
	}

	.feature-list a:hover h3 {
		color: var(--accent);
	}

	.feature-list p {
		margin: 0;
		font-size: 0.93rem;
		color: var(--ink-dim);
	}

	.ahead {
		margin: 1.5rem 0 0;
		font-size: 0.93rem;
		color: var(--ink-dim);
	}

	.tiers {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 22rem), 1fr));
		gap: 2rem 4rem;
		padding-block: 3rem 5rem;
		border-top: 1px solid var(--line);
	}

	.desktop-head h2,
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
