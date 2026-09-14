// The mountain scene behind the home page, composed the way
// crates/alpymist-ui/src/backdrop.rs composes the splash: a sky gradient,
// ridges hazed towards the horizon by distance, mist lying between them, and
// the furthest range quietly repeating the mark.
//
// Drawn once at build time into SVG paths, from a fixed seed, so the page is
// the same on every load and costs the browser nothing to compute.

import { elevation, UNIT } from './mark';

export const WIDTH = 1600;
export const HEIGHT = 900;

type Rgb = [number, number, number];

const hex = (c: string): Rgb => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16)) as Rgb;
const css = ([r, g, b]: Rgb) => `rgb(${r} ${g} ${b})`;

/** Blend towards `other` by `amount` percent, as `Rgb::mix`. */
function mix(a: Rgb, b: Rgb, amount: number): Rgb {
	const t = Math.min(amount, 100) / 100;
	return a.map((v, i) => Math.round(v * (1 - t) + b[i] * t)) as Rgb;
}

/** mulberry32: a few lines, and adequate for scenery. */
function rng(seed: number) {
	let s = seed >>> 0;
	return () => {
		s = (s + 0x6d2b79f5) >>> 0;
		let t = s;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

/** Midpoint displacement over `2^levels` segments, as `Ridge::generate`. */
function ridge(next: () => number, amplitude: number, roughness: number, levels: number) {
	const n = 1 << levels;
	const h = new Array<number>(n + 1).fill(0);
	h[0] = (next() - 0.5) * amplitude;
	h[n] = (next() - 0.5) * amplitude;
	let step = n;
	let amp = amplitude;
	while (step > 1) {
		const half = step / 2;
		for (let i = half; i < n; i += step) {
			h[i] = (h[i - half] + h[i + half]) / 2 + (next() - 0.5) * amp;
		}
		amp *= roughness;
		step = half;
	}
	return h;
}

function toPath(heights: number[]): string {
	const dx = WIDTH / (heights.length - 1);
	const top = heights.map((y, i) => `L${(i * dx).toFixed(1)} ${y.toFixed(1)}`).join('');
	return `M0 ${HEIGHT}${top}L${WIDTH} ${HEIGHT}Z`;
}

export interface Layer {
	kind: 'ridge' | 'mist';
	d?: string;
	y?: number;
	height?: number;
	fill: string;
	opacity?: number;
}

export const palette = {
	skyHigh: '#0b121e',
	skyLow: '#3a4c63',
	ridgeNear: '#070c14',
	mist: '#afc2d6'
};

export function scene(seed = 2026): Layer[] {
	const next = rng(seed);
	const near = hex(palette.ridgeNear);
	const low = hex(palette.skyLow);
	const total = 4;
	const layers: Layer[] = [];

	for (let depth = total; depth >= 0; depth--) {
		const base = HEIGHT * (0.5 + (total - depth) * 0.085);
		const noise = ridge(next, 180 - depth * 18, 0.52, 7);
		const heights = noise.map((n, i) => {
			let y = base + n * (0.5 + (total - depth) * 0.12);
			if (depth === total) {
				// The furthest range is the mark, stretched across the right of the
				// scene with terrain noise on top, so it echoes rather than stamps.
				const x = (i / (noise.length - 1)) * WIDTH;
				const u = ((x - WIDTH * 0.42) / (WIDTH * 0.52)) * UNIT;
				y -= (elevation(u) / UNIT) * HEIGHT * 0.3;
			}
			return Math.min(y, HEIGHT);
		});
		const haze = (depth * 70) / total;
		layers.push({ kind: 'ridge', d: toPath(heights), fill: css(mix(near, low, haze)) });

		if (depth > 0) {
			layers.push({
				kind: 'mist',
				y: base - 30,
				height: 130,
				fill: palette.mist,
				opacity: 0.05 + depth * 0.012
			});
		}
	}
	return layers;
}
