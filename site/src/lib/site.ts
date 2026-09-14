export const REPO = 'https://github.com/bisand/alpymist';
export const PKGS = 'https://pkgs.alpymist.org';
export const ALPINE_VERSION = 'v3.24';
export const KEY_NAME = 'alpymist-2026.rsa';

export const nav = [
	{ href: '/', label: 'Home' },
	{ href: '/download/', label: 'Download' },
	{ href: '/docs/', label: 'Docs' }
];

/** ADR 0001's tiers, most capable first. */
export const tiers = [
	{
		name: 'Full',
		backend: 'Hyprland',
		when: 'Accelerated DRM, GL ES 3.2 or newer, 3 GiB of memory',
		note: 'Animations, blur, Ghostty as the terminal.'
	},
	{
		name: 'Lite',
		backend: 'labwc on GLES2',
		when: 'Accelerated DRM, GL ES 2.0 or newer, 1.5 GiB of memory',
		note: 'A stacking desktop that still uses the GPU.'
	},
	{
		name: 'Potato',
		backend: 'labwc on pixman',
		when: 'Kernel modesetting, but no GPU worth rendering with',
		note: 'Wayland composited entirely on the CPU.'
	},
	{
		name: 'Legacy',
		backend: 'X11 and i3',
		when: 'No DRM/KMS device at all',
		note: 'A fallback that still boots to a usable session.'
	}
];
