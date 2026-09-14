<script lang="ts">
	// A block of commands, with the prompt drawn but not copied.
	let { lines, label = 'sh' }: { lines: string[]; label?: string } = $props();

	let copied = $state(false);

	async function copy() {
		try {
			await navigator.clipboard.writeText(lines.join('\n'));
			copied = true;
			setTimeout(() => (copied = false), 1600);
		} catch {
			// No clipboard permission; the text is still selectable.
		}
	}
</script>

<div class="shell">
	<div class="top">
		<span>{label}</span>
		<button type="button" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
	</div>
	<pre><code
			>{#each lines as line, i (i)}<span class="line"
					><span class="prompt" aria-hidden="true">❯ </span>{line}</span
				>{'\n'}{/each}</code
		></pre>
</div>

<style>
	.shell {
		border: 1px solid var(--line);
		border-radius: var(--radius);
		background: var(--ridge);
		overflow: hidden;
		margin: 1.25rem 0;
	}

	.top {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.35rem 0.5rem 0.35rem 1rem;
		border-bottom: 1px solid var(--line);
		font-family: var(--mono);
		font-size: 0.75rem;
		color: var(--ink-dim);
	}

	button {
		font: inherit;
		color: var(--ink-dim);
		background: transparent;
		border: 1px solid var(--line-strong);
		border-radius: 5px;
		padding: 0.15rem 0.6rem;
		cursor: pointer;
	}

	button:hover {
		color: var(--ink);
		border-color: var(--accent-deep);
	}

	pre {
		margin: 0;
		border: 0;
		border-radius: 0;
		font-size: 0.875rem;
	}

	.prompt {
		color: var(--accent);
		user-select: none;
	}
</style>
