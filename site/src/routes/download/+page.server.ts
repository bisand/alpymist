import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { KEY_NAME } from '$lib/site';

// The fingerprint is taken from the key alpymist-keys ships, at build time, so
// the page cannot go on showing an old one after the key is rotated.
export function load() {
	const key = readFileSync(resolve(process.cwd(), `../aports/alpymist-keys/${KEY_NAME}.pub`));
	return { fingerprint: createHash('sha256').update(key).digest('hex') };
}
