export const REPO = 'https://github.com/bisand/alpymist';
export const PKGS = 'https://pkgs.alpymist.org';
export const ALPINE_VERSION = 'v3.24';
export const KEY_NAME = 'alpymist-2026.rsa';

export const nav = [
	{ href: '/', label: 'Home' },
	{ href: '/download/', label: 'Download' },
	{ href: '/docs/', label: 'Docs' }
];

/** What the probe says of Hyprland on a machine, best first (ADR 0001's addendum). */
export const verdicts = [
	{
		name: 'Runs',
		backend: 'Hyprland on the GPU',
		when: 'Accelerated DRM, GL ES 3.2 or newer, 3 GiB of memory',
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
		when: 'No DRM/KMS device, only a firmware framebuffer, or GL ES below 3.2',
		note: 'The installer says so, with the reasons, and lets you go on.'
	}
];
