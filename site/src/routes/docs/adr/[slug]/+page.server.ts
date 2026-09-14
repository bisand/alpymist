import { error } from '@sveltejs/kit';
import { adrs, render } from '$lib/docs';

export function entries() {
	return adrs.map(({ slug }) => ({ slug }));
}

export function load({ params }) {
	const index = adrs.findIndex((a) => a.slug === params.slug);
	if (index < 0) error(404, 'No such decision record');
	const { markdown, ...adr } = adrs[index];
	const neighbour = (i: number) => (adrs[i] ? { slug: adrs[i].slug, title: adrs[i].title } : null);
	return {
		adr,
		html: render(markdown),
		previous: neighbour(index - 1),
		next: neighbour(index + 1)
	};
}
