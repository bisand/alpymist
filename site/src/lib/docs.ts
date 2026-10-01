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

/**
 * Links between ADRs and between chapters of the manual stay on the site;
 * anything else in the repository goes to GitHub. `dir` is where the file
 * holding the link lives, under docs/.
 */
function rewrite(href: string, dir: string): string {
	if (/^[a-z]+:|^#|^\//i.test(href)) return href;
	const url = new URL(href, `https://repo/docs/${dir}/`);
	const resolved = url.pathname.slice(1);
	const adr = resolved.match(/^docs\/adr\/(\d{4}-[\w-]+)\.md$/);
	if (adr) return `/docs/adr/${adr[1]}/${url.hash}`;
	const chapter = resolved.match(/^docs\/manual\/\d+-([\w-]+)\.md$/);
	if (chapter) return `/manual/${chapter[1]}/${url.hash}`;
	return `${REPO}/blob/main/${resolved}${url.hash}`;
}

/** What a heading is called in an address: its words, lower case, hyphenated. */
export function anchor(text: string): string {
	return text
		.toLowerCase()
		.replace(/<[^>]+>/g, '')
		// An apostrophe arrives as itself from the source and as an entity from
		// the rendered heading; neither belongs in an address.
		.replace(/&#?\w+;|['’`]/g, '')
		.replace(/[^\p{L}\p{N}]+/gu, '-')
		.replace(/^-|-$/g, '');
}

function renderer(dir: string): Marked {
	return new Marked({
		gfm: true,
		walkTokens(token) {
			if (token.type === 'link') token.href = rewrite(token.href, dir);
		},
		renderer: {
			// Headings get an id, so a page can link to its own sections.
			heading({ tokens, depth }) {
				const html = this.parser.parseInline(tokens);
				return `<h${depth} id="${anchor(html)}">${html}</h${depth}>\n`;
			}
		}
	});
}

const renderers = { adr: renderer('adr'), manual: renderer('manual') };

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

export function render(markdown: string, dir: keyof typeof renderers = 'adr'): string {
	return renderers[dir].parse(markdown, { async: false });
}
