export const prerender = true;

// Every page is written as dir/index.html. GitHub Pages sends /docs to a docs/
// directory when one exists, so docs.html beside it would never be served.
export const trailingSlash = 'always';
