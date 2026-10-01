<script lang="ts">
	import { HEIGHT, WIDTH, palette, scene } from '$lib/scenery';

	import milkyWay from '$lib/assets/milky-way.webp';

	// With `picture`, Alpymist's own picture — the one the boot menus, the
	// splash, the login screen and the lock show — is laid over the drawn
	// mountains, which stay underneath for as long as it takes to arrive, or if
	// it never does. The desktop's screens fall back the same way.
	// lib/assets/milky-way.webp is brand/wallpapers/milky-way.jpg, recompressed.
	let { picture = false }: { picture?: boolean } = $props();

	const layers = scene();
</script>

<svg
	class="backdrop"
	viewBox="0 0 {WIDTH} {HEIGHT}"
	preserveAspectRatio="xMidYMax slice"
	aria-hidden="true"
>
	<defs>
		<linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" stop-color={palette.skyHigh} />
			<stop offset="0.75" stop-color={palette.skyLow} />
		</linearGradient>
		<linearGradient id="mist" x1="0" y1="0" x2="0" y2="1">
			<stop offset="0" stop-color={palette.mist} stop-opacity="0" />
			<stop offset="0.5" stop-color={palette.mist} stop-opacity="1" />
			<stop offset="1" stop-color={palette.mist} stop-opacity="0" />
		</linearGradient>
	</defs>
	<rect width={WIDTH} height={HEIGHT} fill="url(#sky)" />
	{#each layers as layer, i (i)}
		{#if layer.kind === 'ridge'}
			<path d={layer.d} fill={layer.fill} />
		{:else}
			<rect
				class="mist"
				style="--drift: {i % 2 ? -1 : 1}"
				x="-80"
				y={layer.y}
				width={WIDTH + 160}
				height={layer.height}
				fill="url(#mist)"
				opacity={layer.opacity}
			/>
		{/if}
	{/each}
</svg>
{#if picture}
	<img class="backdrop picture" src={milkyWay} alt="" width="1920" height="1080" />
	<div class="backdrop shade"></div>
{/if}

<style>
	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
	}

	.picture {
		object-fit: cover;
		object-position: 62% 50%;
	}

	/* Keeps the words on the left readable, and lets the picture's foot sink
	   into the page instead of ending at an edge. */
	.shade {
		background:
			linear-gradient(to top, var(--sky-high), transparent 22%),
			linear-gradient(
				to right,
				color-mix(in srgb, var(--sky-high) 72%, transparent),
				transparent 62%
			);
	}

	.mist {
		animation: drift 38s ease-in-out infinite alternate;
	}

	@keyframes drift {
		to {
			transform: translateX(calc(var(--drift) * 60px));
		}
	}
</style>
