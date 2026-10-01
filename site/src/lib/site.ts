export const REPO = 'https://github.com/bisand/alpymist';
export const RELEASES = `${REPO}/releases/latest`;
export const PKGS = 'https://pkgs.alpymist.org';
export const DEV_PKGS = 'https://dev.pkgs.alpymist.org';
export const ALPINE_VERSION = 'v3.24';
export const KEY_NAME = 'alpymist-2026.rsa';

export const nav = [
	{ href: '/', label: 'Home' },
	{ href: '/download/', label: 'Download' },
	{ href: '/docs/', label: 'Docs' }
];

/** What the probe says of Hyprland on a machine, best first (ADR 0001's addenda). */
export const verdicts = [
	{
		name: 'Runs',
		backend: 'Hyprland on the GPU',
		when: 'Accelerated DRM, GL ES 3.0 or newer, 3 GiB of memory',
		note: 'Animations, blur, everything as it is meant to be.'
	},
	{
		name: 'Slow',
		backend: 'Hyprland on the processor',
		when: 'A software renderer such as llvmpipe, or less than 3 GiB of memory',
		note: 'It runs, and the probe says why it will not be quick.'
	},
	{
		name: 'Unlikely',
		backend: 'Hyprland may not start',
		when: 'No DRM/KMS device, only a firmware framebuffer, no EGL, or GL ES below 3.0',
		note: 'The installer says so, with the reasons, and lets you go on.'
	}
];

/** What the desktop is made of, each with the decision record that explains it. */
export const features = [
	{
		title: 'Settings, three ways in',
		body: 'One library behind a Settings app, the alpymist command line and the popups under the bar. Every setting the app shows, alpymist list shows too.',
		adr: '0007-settings'
	},
	{
		title: 'The lock is the login screen',
		body: 'The same picture, clock and card, as a session lock the compositor keeps up even if the program drawing it dies.',
		adr: '0010-lock-screen'
	},
	{
		title: 'Screensavers are packages',
		body: 'A program and a file declaring its settings. Mountains and a starfield ship; anyone can package another without touching Alpymist.',
		adr: '0009-screensaver-and-idle'
	},
	{
		title: 'A layout for each set of screens',
		body: 'Arrange them in Settings, and the dock, the projector and the lid each get their layout back. Every screen has its own workspaces 1 to 9.',
		adr: '0015-screens'
	},
	{
		title: 'A keyring, and an SSH agent that forgets',
		body: 'Secrets are kept in a keyring the login password unlocks. SSH passphrases are asked for in a prompt Ctrl+Alt+Delete vouches for.',
		adr: '0013-keyring-and-ssh-agent'
	},
	{
		title: 'Copy and paste on Super',
		body: 'Super+C, X and V work in every window, terminals included. A clipboard history exists, and is off until you turn it on.',
		adr: '0014-clipboard'
	},
	{
		title: 'Docks are asked about',
		body: 'A Thunderbolt or USB4 device reaches memory, so it is let in only when you approve it, with the IOMMU on.',
		adr: '0012-thunderbolt-and-usb4'
	},
	{
		title: 'Fingerprints unlock, never log in',
		body: 'Once an administrator turns it on, an enrolled finger unlocks the screen and answers admin prompts. Logging in still takes the password.',
		adr: '0016-fingerprints'
	},
	{
		title: 'AI usage in the bar',
		body: 'Limits and spend for the AI tools you pay for. Providers are packages, keys live in the keyring, and nothing is on until you add one.',
		adr: '0017-ai-usage'
	},
	{
		title: 'Graphics in a virtual machine',
		body: 'In a virtio-gpu guest the installer offers a Mesa with the virgl driver, so Hyprland draws on the host\'s graphics card and not the processor.',
		adr: '0018-guest-graphics'
	}
];
