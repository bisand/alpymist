import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		// Every page is prerendered to plain HTML for GitHub Pages. The fallback
		// is what Pages serves for an address nothing was rendered at.
		adapter: adapter({ fallback: '404.html' }),
		prerender: {
			handleHttpError: 'fail',
			handleMissingId: 'fail'
		}
	}
};

export default config;
