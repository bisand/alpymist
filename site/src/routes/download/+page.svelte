<script lang="ts">
	import Shell from '$lib/components/Shell.svelte';
	import { ALPINE_VERSION, KEY_NAME, PKGS, REPO } from '$lib/site';

	let { data } = $props();
</script>

<svelte:head>
	<title>Download — Alpymist</title>
	<meta
		name="description"
		content="Add the signed Alpymist package repository to Alpine Linux, or build the Alpymist ISO yourself."
	/>
</svelte:head>

<div class="wrap page">
	<header class="intro">
		<p class="eyebrow">Download</p>
		<h1>Get Alpymist</h1>
		<p>
			There are two ways onto an Alpymist system, and neither is a script piped into a shell. Add
			the signed repository to an Alpine machine you already have, or build the installer image and
			boot it.
		</p>
		<p class="warning" role="note">
			<strong>Pre-alpha.</strong> Expect breakage. Try it in a virtual machine or on hardware you do not
			need tomorrow.
		</p>
	</header>

	<div class="options">
		<section class="option" aria-labelledby="repo">
			<p class="badge live">Available</p>
			<h2 id="repo">On an existing Alpine {ALPINE_VERSION} system</h2>
			<p>
				Trust the Alpymist key, add the repository and refresh the index. Alpymist is built against
				Alpine {ALPINE_VERSION}, and the repository only serves that release.
			</p>
			<Shell
				lines={[
					`doas wget -O /etc/apk/keys/${KEY_NAME}.pub ${PKGS}/${KEY_NAME}.pub`,
					`echo ${PKGS}/${ALPINE_VERSION}/alpymist | doas tee -a /etc/apk/repositories`,
					'doas apk update'
				]}
			/>
			<p>Ask the probe which tier the machine can run, then add that tier's desktop:</p>
			<Shell lines={['doas apk add alpymist', 'alpymist probe']} />
			<div class="table-scroll">
				<table>
					<thead><tr><th>Probe says</th><th>Install</th></tr></thead>
					<tbody>
						<tr><td>Full</td><td><code>alpymist-desktop-full</code></td></tr>
						<tr><td>Lite or Potato</td><td><code>alpymist-desktop-lite</code></td></tr>
						<tr><td>Legacy</td><td><code>alpymist-desktop-legacy</code></td></tr>
					</tbody>
				</table>
			</div>
			<p>
				The ISO's installer also enables <code>dbus</code>, <code>seatd</code> and
				<code>greetd</code> and points the login screen at the right session. On an existing system
				that part is up to you for now.
			</p>

			<h3>Check the key before you trust it</h3>
			<p>
				The downloaded key should have this SHA-256 fingerprint, which matches the copy in the source
				repository at
				<a href="{REPO}/blob/main/aports/alpymist-keys/{KEY_NAME}.pub"
					><code>aports/alpymist-keys</code></a
				>:
			</p>
			<Shell lines={[`sha256sum /etc/apk/keys/${KEY_NAME}.pub`]} />
			<p class="fingerprint"><code>{data.fingerprint}</code></p>
		</section>

		<section class="option" aria-labelledby="iso">
			<p class="badge soon">Build it yourself</p>
			<h2 id="iso">Bootable ISO</h2>
			<p>
				Signed images are not published yet. CI builds an x86_64 ISO every night and boots it in
				QEMU, and you can build the same image locally. You need Docker and a Rust toolchain.
			</p>
			<Shell
				lines={[
					'git clone https://github.com/bisand/alpymist.git',
					'cd alpymist',
					'make iso ARCH=x86_64',
					'make smoke ARCH=x86_64'
				]}
			/>
			<p>
				<code>make iso</code> builds the packages inside an Alpine container and assembles an image
				in <code>out/</code>. <code>make smoke</code> boots it and checks that it reports a desktop tier.
				Use <code>ARCH=aarch64</code> for ARM machines and Apple Silicon virtual machines.
			</p>
			<p><a href="/docs/building/">More on building from source →</a></p>
		</section>
	</div>

	<section class="why" aria-labelledby="why">
		<h2 id="why">Why only signed packages</h2>
		<p>
			Only the repository index is signed, by hand, with a key CI never sees. The index pins the hash
			of every package, so a compromised build can produce a bad artifact but never an update that
			an installed system will accept. The reasoning is in
			<a href="/docs/adr/0002-supply-chain/">ADR 0002</a>.
		</p>
	</section>
</div>

<style>
	.page {
		padding-block: 3.5rem 5rem;
	}

	.intro {
		max-width: var(--measure);
	}

	h1 {
		font-size: clamp(2rem, 4.5vw, 3rem);
		margin: 0.5rem 0 1rem;
	}

	.intro p:not(.eyebrow) {
		color: var(--ink-dim);
		font-size: 1.1rem;
	}

	.warning {
		border-left: 3px solid var(--amber);
		background: color-mix(in srgb, var(--amber) 8%, transparent);
		padding: 0.75rem 1rem;
		border-radius: 0 6px 6px 0;
		font-size: 1rem !important;
	}

	.warning strong {
		color: var(--amber);
	}

	.options {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 28rem), 1fr));
		gap: 1.5rem;
		margin-top: 3rem;
	}

	.option {
		min-width: 0;
		padding: clamp(1.25rem, 3vw, 2rem);
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--surface);
	}

	.option h2 {
		font-size: 1.4rem;
		margin: 0.75rem 0;
	}

	.option h3 {
		font-size: 1.05rem;
		margin: 2rem 0 0.5rem;
	}

	.option p {
		color: var(--ink-dim);
	}

	.badge {
		display: inline-block;
		margin: 0;
		font-family: var(--mono);
		font-size: 0.75rem;
		padding: 0.15rem 0.55rem;
		border-radius: 999px;
		border: 1px solid;
	}

	.live {
		color: var(--green) !important;
		border-color: color-mix(in srgb, var(--green) 45%, transparent);
	}

	.soon {
		color: var(--amber) !important;
		border-color: color-mix(in srgb, var(--amber) 45%, transparent);
	}

	.fingerprint code {
		display: block;
		overflow-wrap: anywhere;
		padding: 0.6rem 0.8rem;
		font-size: 0.85rem;
		color: var(--green);
	}

	.why {
		max-width: var(--measure);
		margin-top: 4rem;
	}

	.why h2 {
		font-size: 1.4rem;
	}

	.why p {
		color: var(--ink-dim);
	}
</style>
