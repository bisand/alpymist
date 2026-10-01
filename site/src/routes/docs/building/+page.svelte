<script lang="ts">
	import Shell from '$lib/components/Shell.svelte';
	import { REPO } from '$lib/site';

	const layout = [
		[
			'crates/',
			'Rust workspace: the alpymist command line and settings library, the hardware probe, the installer, splash, login and lock screens, Settings, the menu, the bar\'s popups and the screensavers'
		],
		['aports/', 'APKBUILDs for the Alpymist packages'],
		['desktop/', 'What the desktop package installs: Hyprland, bar and terminal configuration for /etc/skel, PAM and polkit rules, and the screensaver and AI usage definitions'],
		['profiles/', 'mkimage profiles and genapkovl overlays'],
		['builder/', 'The Alpine container everything is built in'],
		['ci/', 'The scripts CI and make run to build packages and images, and how publishing works'],
		['xtask/', 'Version, smoke-test and publish orchestration, in Rust'],
		['docs/manual/', 'The manual, one file a chapter'],
		['docs/adr/', 'Architecture decision records'],
		['site/', 'This web site']
	];
</script>

<svelte:head>
	<title>Building from source — Alpymist</title>
	<meta
		name="description"
		content="Build the Alpymist Rust workspace, packages and bootable ISO, and boot it in QEMU."
	/>
</svelte:head>

<article class="prose">
	<p class="eyebrow">Guide</p>
	<h1>Building from source</h1>
	<p>
		Everything Alpymist ships is built in an Alpine container, so the binaries link against the same
		musl they run on. You need <strong>Docker</strong> for that and a <strong>Rust toolchain</strong>
		for the workspace; <code>rust-toolchain.toml</code> pins the version to match Alpine's.
	</p>

	<h2>Get the code</h2>
	<Shell lines={['git clone https://github.com/bisand/alpymist.git', 'cd alpymist', 'make help']} />

	<h2>The Rust workspace</h2>
	<p>
		The same checks CI runs on every pull request. Formatting and clippy warnings are errors.
	</p>
	<Shell lines={['make check', 'make test', 'make lint']} />
	<p>
		CI itself runs clippy and the tests with the <code>alpymist-ui/render</code> feature, lints the
		installer, splash and login screen once more as they ship, on DRM, and checks that every
		package's version equals the workspace's. The exact commands are in
		<a href="{REPO}/blob/main/CLAUDE.md"><code>CLAUDE.md</code></a>.
	</p>
	<p>On any Linux machine you can ask the probe how Hyprland will do on the hardware in front of you:</p>
	<Shell lines={['cargo run -p alpymist -- probe']} />
	<p>
		The installer opens in a window during development, so you can walk through it without building
		an image. <code>ALPYMIST_DRY_RUN=1</code> makes the last step report what it would run instead
		of touching a disk.
	</p>
	<Shell lines={['ALPYMIST_DRY_RUN=1 cargo run -p alpymist-install']} />

	<h2>A bootable image</h2>
	<p>
		<code>ARCH</code> defaults to your machine's architecture. x86_64 is the architecture Alpymist
		actually targets; aarch64 is what it is developed on.
	</p>
	<Shell lines={['make iso ARCH=x86_64', 'make smoke ARCH=x86_64']} />
	<p>
		<code>make iso</code> builds the builder container, the packages and the image, and leaves the
		ISO in <code>out/</code>. <code>make smoke</code> boots it under QEMU, reads the serial console,
		and checks that the first-boot probe said how Hyprland will do and explained itself.
	</p>

	<p>
		How the packages, images and channels are built and published is written down in
		<a href="{REPO}/blob/main/ci/README.md"><code>ci/README.md</code></a>.
	</p>

	<h2>Repository layout</h2>
	<div class="table-scroll">
		<table>
			<thead><tr><th>Path</th><th>Contents</th></tr></thead>
			<tbody>
				{#each layout as [path, what] (path)}
					<tr><td><code>{path}</code></td><td>{what}</td></tr>
				{/each}
			</tbody>
		</table>
	</div>

	<h2>This site</h2>
	<p>
		The site is SvelteKit, prerendered to static HTML and published to GitHub Pages when
		<code>site/</code>, <code>docs/</code> or the signing key in <code>aports/alpymist-keys</code>
		changes on <code>main</code>. The decision records are read straight from
		<code>docs/adr</code>, so editing one there updates it here.
	</p>
	<Shell lines={['cd site', 'npm ci', 'npm run dev']} />

	<p>
		Contributions are accepted under MIT or Apache-2.0. Security issues go through
		<a href="{REPO}/blob/main/SECURITY.md">SECURITY.md</a>, not the issue tracker.
	</p>
</article>

<style>
	h1 {
		font-size: clamp(1.9rem, 4vw, 2.6rem);
		margin: 0.5rem 0 1rem;
	}

	td:first-child {
		white-space: nowrap;
	}
</style>
