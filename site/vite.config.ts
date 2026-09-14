import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// The docs pages are read from the repository's own docs/, one level up,
		// so the site never carries a copy that can drift from them.
		fs: { allow: ['..'] }
	}
});
