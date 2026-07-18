import { describe, expect, it } from 'vitest';

import { mailBodyText, quoteMailBody, uniqueMailAddresses } from './compose';

describe('mail compose helpers', () => {
	it('quotes reply bodies line by line', () => {
		expect(quoteMailBody('first\nsecond')).toBe('> first\n> second');
	});

	it('converts sanitized html bodies to text for replies and forwards', () => {
		expect(
			mailBodyText({ type: 'html', content: '<p>Hello&nbsp;&amp;&nbsp;bye</p><br>Next' })
		).toBe('Hello & bye\n\nNext');
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
