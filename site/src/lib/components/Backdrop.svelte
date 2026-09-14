<script lang="ts">
	import { HEIGHT, WIDTH, palette, scene } from '$lib/scenery';

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

<style>
	.backdrop {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
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
