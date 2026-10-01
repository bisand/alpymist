import { chapters } from '$lib/manual';

// Titles and groups for the chapter list; each chapter's body is rendered
// into its own page at build time.
export function load() {
	return {
		chapters: chapters.map(({ slug, title, group, summary }) => ({ slug, title, group, summary }))
	};
}
