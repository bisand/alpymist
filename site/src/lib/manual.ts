// The manual, read from the repository's docs/manual at build time, as the
// decision records are. A chapter is a file `NN-slug.md`: the number orders
// it, the first heading names it, and a `<!-- group: … -->` comment says
// which part of the manual it belongs to.

import { anchor } from './docs';

const sources = import.meta.glob('../../../docs/manual/*.md', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

export interface Chapter {
	/** The file's name without `.md`, for the link to its source. */
	file: string;
	slug: string;
	title: string;
	group: string;
	summary: string;
	/** The second-level headings, for "On this page". */
	sections: { id: string; title: string }[];
	markdown: string;
}

function plain(text: string): string {
	return text
		.replace(/<[^>]+>/g, '')
		.replace(/[*_`]/g, '')
		.replace(/\[([^\]]+)\]\([^)]+\)/g, '$1');
}

function parse(path: string, source: string): Chapter {
	const file = path.split('/').pop()!.replace(/\.md$/, '');
	const slug = file.replace(/^\d+-/, '');
	const title = plain(source.match(/^#\s+(.+)$/m)?.[1] ?? slug);
	const group = source.match(/<!--\s*group:\s*(.+?)\s*-->/)?.[1] ?? 'Manual';
	const markdown = source
		.replace(/^#\s+.+\n/m, '')
		.replace(/<!--\s*group:.*?-->\n?/, '')
		.trim();
	const first = markdown.split(/\n\s*\n/).find((p) => !/^[#|<`]/.test(p)) ?? '';
	const sections = [...markdown.matchAll(/^##\s+(.+)$/gm)].map((m) => ({
		id: anchor(m[1].replace(/`/g, '')),
		title: plain(m[1])
	}));
	return { file, slug, title, group, summary: plain(first).replace(/\s+/g, ' '), sections, markdown };
}

export const chapters: Chapter[] = Object.entries(sources)
	.sort(([a], [b]) => a.localeCompare(b))
	.map(([path, source]) => parse(path, source));

/** The chapters under their groups, in the order the groups first appear. */
export function grouped<T extends { group: string }>(list: T[]): { group: string; chapters: T[] }[] {
	const groups: { group: string; chapters: T[] }[] = [];
	for (const chapter of list) {
		const last = groups.find((g) => g.group === chapter.group);
		if (last) last.chapters.push(chapter);
		else groups.push({ group: chapter.group, chapters: [chapter] });
	}
	return groups;
}
