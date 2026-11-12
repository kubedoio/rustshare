import { describe, expect, it } from 'vitest';

import { mailBodyText, quoteMailBody, uniqueMailAddresses } from './compose';

describe('mail compose helpers', () => {
	it('quotes reply bodies line by line', () => {
		expect(quoteMailBody('first\nsecond')).toBe('> first\n> second');
	});

	it('converts sanitized html bodies to text for replies and forwards', () => {
		expect(
			mailBodyText({ type: 'html', content: '<p>Hello&nbsp;&amp;&nbsp;bye</p><br>Next' })
		).toBe('Hello &amp; bye\n\nNext');
	});

	it('keeps encoded HTML and plain-text markup inert in the rich-text reply body', () => {
		expect(
			mailBodyText({
				type: 'html',
				content: '&amp;lt;img src="https://tracker.example/pixel"&amp;gt;'
			})
		).toBe('&amp;lt;img src="https://tracker.example/pixel"&amp;gt;');
		expect(
			mailBodyText({ type: 'text', content: '<img src="https://tracker.example/pixel">' })
		).toBe('&lt;img src="https://tracker.example/pixel"&gt;');
	});

	it('deduplicates reply-all recipients and excludes the current identity', () => {
		expect(
			uniqueMailAddresses(
				['sender@example.com', 'ME@example.com', 'sender@example.com', 'other@example.com'],
				['me@example.com']
			)
		).toEqual(['sender@example.com', 'other@example.com']);
	});
});
