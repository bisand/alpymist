<script lang="ts">
	import Shell from '$lib/components/Shell.svelte';
	import { ALPINE_VERSION, DEV_PKGS, KEY_NAME, PKGS, RELEASES, REPO } from '$lib/site';

	let { data } = $props();
</script>

<svelte:head>
	<title>Download — Alpymist</title>
	<meta
		name="description"
		content="Download the Alpymist installer image, or add the signed package repository to Alpine Linux. How upgrades and the stable and dev channels work."
	/>
</svelte:head>

<div class="wrap page">
	<header class="intro">
		<p class="eyebrow">Download</p>
		<h1>Get Alpymist</h1>
		<p>
			There are two ways onto an Alpymist system, and neither is a script piped into a shell. Boot
			the installer image, or add the signed repository to an Alpine machine you already have.
		</p>
		<p class="warning" role="note">
			<strong>Young software.</strong> It installs and it runs, and it will have rough edges. Try it in
			a virtual machine, or on hardware you do not need tomorrow.
		</p>
	</header>

	<div class="options">
		<section class="option" aria-labelledby="iso">
			<p class="badge live">Available</p>
			<h2 id="iso">Installer image</h2>
			<p>
				Every release has an ISO for each architecture attached, with its SHA-256 checksum:
				<strong>x86_64</strong> for laptops and PCs, and <strong>aarch64</strong> for virtual
				machines on Apple silicon Macs, such as UTM.
			</p>
			<p><a class="button" href={RELEASES}>Download from the latest release →</a></p>
			<p>Check the download against its checksum, with both files in the same directory:</p>
			<Shell lines={['sha256sum -c alpymist-*.iso.sha256']} />
			<p>
				The checksum is published beside the image, so it says the download is intact, not who
				made it. The packages an installed system upgrades from are signed; the image is not yet.
			</p>

			<h3>What the installer asks</h3>
			<p>
				Keyboard, region, network, which disk and whether to encrypt it, and the first account.
				It then says how Hyprland will do on the machine, and why. Nothing is written to the disk
				until you confirm on the last screen. The <code>root</code> account is locked, and
				administration is through <code>doas</code> with your password.
			</p>
			<p>
				In a virtual machine with a virtio-gpu display, the installer offers to draw with the
				host's graphics card. That takes a Mesa from a second repository, which is
				<a href="/docs/adr/0018-guest-graphics/">its own decision</a> and can be declined.
			</p>

			<h3>Or build it yourself</h3>
			<p>
				<code>make iso</code> builds the same image in an Alpine container, and
				<code>make smoke</code> boots it in QEMU.
				<a href="/docs/building/">Building from source →</a>
			</p>
		</section>

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
			<p>Ask the probe how Hyprland will run on the machine, then add the desktop:</p>
			<Shell lines={['doas apk add alpymist', 'alpymist probe', 'doas apk add alpymist-desktop']} />
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
	</div>

	<section class="more" aria-labelledby="channels">
		<h2 id="channels">Upgrades and channels</h2>
		<p>
			An image is for installing, not for upgrading. An installed system upgrades with
			<code>doas apk upgrade -U</code>, or Update in the menu, from the channel it follows:
		</p>
		<div class="table-scroll">
			<table>
				<thead><tr><th>Channel</th><th>Moves</th><th>Repository</th></tr></thead>
				<tbody>
					<tr>
						<td><strong>stable</strong></td>
						<td>At each release. What the installer sets up.</td>
						<td><code>{PKGS}/{ALPINE_VERSION}/alpymist</code></td>
					</tr>
					<tr>
						<td><strong>dev</strong></td>
						<td>On every push to <code>main</code>.</td>
						<td><code>{DEV_PKGS}/{ALPINE_VERSION}/alpymist</code></td>
					</tr>
				</tbody>
			</table>
		</div>
		<Shell
			lines={[
				'alpymist channel',
				'doas alpymist channel dev',
				'doas alpymist channel stable'
			]}
		/>
		<p>
			The first says which channel the system follows. Following dev trusts its key and upgrades;
			going back to stable distrusts the key again and downgrades to the release. The dev key is
			shipped where apk does not look, so a system that never asked for dev does not trust it.
			The reasoning is in <a href="/docs/adr/0006-release-channels/">ADR 0006</a>.
		</p>
	</section>

	<section class="more" aria-labelledby="why">
		<h2 id="why">What the signature does and does not say</h2>
		<p>
			The repository's index is signed, and it pins the hash of every package, so apk refuses a
			package that was swapped or changed after the index was made. Stable's index is signed by
			CI, on a Release run of a tagged commit on <code>main</code>: what is signed is what was
			built in the open.
		</p>
		<p>
			What that does not give is a person between a release and your machine. The signing key
			was once kept offline and is now held by GitHub Actions, so anyone following either channel
			is trusting that. <a href="/docs/adr/0002-supply-chain/">ADR 0002</a> and its addenda record
			the trade, and what was given up for it.
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

	.fingerprint code {
		display: block;
		overflow-wrap: anywhere;
		padding: 0.6rem 0.8rem;
		font-size: 0.85rem;
		color: var(--green);
	}

	.button {
		display: inline-flex;
		padding: 0.6rem 1.1rem;
		border-radius: 6px;
		background: var(--accent);
		color: var(--sky-high);
		font-weight: 600;
		text-decoration: none;
	}

	.button:hover {
		background: var(--ink);
	}

	.more {
		max-width: var(--measure);
		margin-top: 4rem;
	}

	.more h2 {
		font-size: 1.4rem;
	}

	.more p {
		color: var(--ink-dim);
	}

	td code {
		overflow-wrap: anywhere;
	}
</style>
