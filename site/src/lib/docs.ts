// The architecture decision records, read from the repository's docs/adr at
// build time. The site renders them; it never keeps its own copy.

import { Marked } from 'marked';
import { REPO } from './site';

const sources = import.meta.glob('../../../docs/adr/*.md', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

export interface Adr {
	slug: string;
	number: string;
	title: string;
	status: string;
	date: string;
	summary: string;
	markdown: string;
}

/** Links between ADRs stay on the site; anything else in the repository goes to GitHub. */
function rewrite(href: string): string {
	if (/^[a-z]+:|^#|^\//i.test(href)) return href;
	const adr = href.match(/^(?:\.\/)?(\d{4}-[\w-]+)\.md(#.*)?$/);
	if (adr) return `/docs/adr/${adr[1]}/${adr[2] ?? ''}`;
	const resolved = new URL(href, 'https://repo/docs/adr/').pathname.slice(1);
	return `${REPO}/blob/main/${resolved}`;
}

const marked = new Marked({
	gfm: true,
	walkTokens(token) {
		if (token.type === 'link') token.href = rewrite(token.href);
	}
});

function parse(path: string, source: string): Adr {
	const slug = path.split('/').pop()!.replace(/\.md$/, '');
	const heading = source.match(/^#\s+(.+)$/m)?.[1] ?? slug;
	const [, number = '', rawTitle = heading] = heading.match(/^ADR\s+(\d+)\s+[—–-]\s+(.+)$/) ?? [];
	// Titles are shown as plain text in navigation and <title>, so drop markdown code marks.
	const title = rawTitle.replace(/`/g, '');
	const status = source.match(/\*\*Status:\*\*\s*([^·\n]+)/)?.[1].trim() ?? '';
	const date = source.match(/\*\*Date:\*\*\s*([^·\n]+)/)?.[1].trim() ?? '';
	// The body without the heading or the status line, which the page shows itself.
	const markdown = source
		.replace(/^#\s+.+\n/m, '')
		.replace(/^\*\*Status:\*\*.*\n/m, '')
		.trim();
	const firstParagraph = markdown
		.split(/\n\s*\n/)
		.find((p) => !p.startsWith('#') && !p.startsWith('|'));
	const summary = (firstParagraph ?? '')
		.replace(/\s+/g, ' ')
		.replace(/[*_`]/g, '')
		.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
	return { slug, number, title, status, date, summary, markdown };
}

export const adrs: Adr[] = Object.entries(sources)
	.map(([path, source]) => parse(path, source))
	.sort((a, b) => a.slug.localeCompare(b.slug));

export function render(markdown: string): string {
	return marked.parse(markdown, { async: false });
}
