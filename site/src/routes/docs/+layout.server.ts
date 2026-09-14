import { adrs } from '$lib/docs';

// Only titles and summaries go to the browser; each record's body is rendered
// into its own page at build time.
export function load() {
	return { adrs: adrs.map(({ markdown: _, ...adr }) => adr) };
}
