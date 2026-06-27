import { describe, it, expect } from 'vitest';
import { splitFrontmatter, wrapFrontmatter, extractH1 } from './frontmatter';

describe('splitFrontmatter', () => {
	it('returns the whole content as body when there is no frontmatter', () => {
		const content = '# Hello\n\nBody text.';
		const result = splitFrontmatter(content);

		expect(result.hasFrontmatter).toBe(false);
		expect(result.frontmatter).toBe('');
		expect(result.body).toBe(content);
	});

	it('splits a document with frontmatter', () => {
		const content = '---\ntitle: My Note\nid: 123\n---\n# Hello\n\nBody text.';
		const result = splitFrontmatter(content);

		expect(result.hasFrontmatter).toBe(true);
		expect(result.frontmatter).toBe('---\ntitle: My Note\nid: 123\n---\n');
		expect(result.body).toBe('# Hello\n\nBody text.');
	});

	it('treats a malformed frontmatter opener as body', () => {
		const content = '---\n# Hello';
		const result = splitFrontmatter(content);

		expect(result.hasFrontmatter).toBe(false);
		expect(result.body).toBe(content);
	});

	it('requires the opening delimiter to be followed by a newline', () => {
		const content = '---# Hello\n\nBody';
		const result = splitFrontmatter(content);

		expect(result.hasFrontmatter).toBe(false);
		expect(result.body).toBe(content);
	});
});

describe('wrapFrontmatter', () => {
	it('preserves body after split/wrap round-trip', () => {
		const original = '---\ntitle: My Note\n---\n# Hello\n\nBody text.';
		const { frontmatter, body } = splitFrontmatter(original);
		const wrapped = wrapFrontmatter(frontmatter, body);

		expect(wrapped).toBe(original);
	});

	it('does not double-wrap a body that already contains frontmatter', () => {
		const frontmatter = '---\ntitle: My Note\n---\n';
		const body = '# Hello\n\nBody text.';
		const wrapped = wrapFrontmatter(frontmatter, body);

		expect(wrapped).toBe('---\ntitle: My Note\n---\n# Hello\n\nBody text.');
		expect(wrapped.match(/---\n/g)?.length).toBe(2);
	});

	it('trims leading blank lines from the body before wrapping', () => {
		const wrapped = wrapFrontmatter('---\ntitle: T\n---\n', '\n\n# Hello');
		expect(wrapped).toBe('---\ntitle: T\n---\n# Hello');
	});
});

describe('extractH1', () => {
	it('returns the first H1 text', () => {
		expect(extractH1('# Title\n\nBody')).toBe('Title');
	});

	it('ignores leading blank lines', () => {
		expect(extractH1('\n\n# Title\nBody')).toBe('Title');
	});

	it('returns null when there is no H1', () => {
		expect(extractH1('Some paragraph\n\n## H2')).toBeNull();
	});

	it('returns null for an empty body', () => {
		expect(extractH1('')).toBeNull();
	});

	it('does not treat non-H1 lines as H1', () => {
		expect(extractH1('#not a heading\n# Real heading')).toBe('Real heading');
	});
});
