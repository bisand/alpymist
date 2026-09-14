// The Alpymist mark, point for point as `MARK` in crates/alpymist-ui/src/logo.rs:
// a high peak left of centre, a lower one to its right, and the saddle between.
// x runs 0-1000 across; y is elevation, 0 at the ground and 1000 at the summit.

export const UNIT = 1000;

/** Height as a percentage of width, as `Silhouette::ASPECT`. */
export const ASPECT = 62;

export const MARK: ReadonlyArray<readonly [number, number]> = [
	[0, 0],
	[80, 90],
	[300, 760],
	[390, 1000],
	[470, 800],
	[560, 560],
	[610, 640],
	[700, 860],
	[760, 720],
	[900, 220],
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
