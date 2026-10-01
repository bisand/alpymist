import { error } from '@sveltejs/kit';
import { render } from '$lib/docs';
import { chapters } from '$lib/manual';

export function entries() {
	return chapters.map(({ slug }) => ({ slug }));
}

export function load({ params }) {
	const index = chapters.findIndex((c) => c.slug === params.slug);
	if (index < 0) error(404, 'No such chapter');
	const { markdown, ...chapter } = chapters[index];
	const neighbour = (i: number) =>
		chapters[i] ? { slug: chapters[i].slug, title: chapters[i].title } : null;
	return {
		chapter,
		html: render(markdown, 'manual'),
		previous: neighbour(index - 1),
		next: neighbour(index + 1)
	};
}
