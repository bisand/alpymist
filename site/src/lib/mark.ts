// The Alpymist small-size mark, point for point as `MARK` in
// crates/alpymist-ui/src/logo.rs: a high peak left of centre, a lower companion
// to its right, and a broad saddle between. Larger brand assets add the mist.
// x runs 0-1000 across; y is elevation, 0 at the ground and 1000 at the summit.

export const UNIT = 1000;

/** Height as a percentage of width, as `Silhouette::ASPECT`. */
export const ASPECT = 62;

export const MARK: ReadonlyArray<readonly [number, number]> = [
	[0, 0],
	[323, 779],
	[448, 1000],
	[686, 362],
	[793, 587],
	[1000, 0]
];

/** Elevation at `x`, linearly interpolated; zero outside the outline. */
export function elevation(x: number): number {
	for (let i = 0; i < MARK.length - 1; i++) {
		const [x0, y0] = MARK[i];
		const [x1, y1] = MARK[i + 1];
		if (x >= x0 && x <= x1) {
			return x1 === x0 ? y0 : y0 + ((x - x0) * (y1 - y0)) / (x1 - x0);
		}
	}
	return 0;
}

/** The mark as a closed SVG path in a `0 0 1000 620` box. */
export function markPath(): string {
	const tall = (UNIT * ASPECT) / 100;
	return (
		MARK.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${(tall - (y * tall) / UNIT).toFixed(1)}`).join(
			''
		) + 'Z'
	);
}

/** A broad negative-space ribbon used in the full-size web mark. */
export function mistPath(): string {
	return (
		'M322.7 137.3C295.5 197.7 325 247.6 406.8 287.1' +
		'C504.5 332.9 531.8 432.8 615.9 476.4C715.9 528.5 854.5 536.8 984.1 620' +
		'H938.6C829.5 559.7 709.1 545.1 604.5 497.2C506.8 451.5 479.5 351.6 388.6 307.9' +
		'C302.3 266.3 261.4 206 290.9 137.3Z'
	);
}
