import { describe, expect, it } from 'vitest';
import { formatObserverError } from '../../../scripts/observer-log.mjs';

describe('observer error logs', () => {
	it('does not include remote error text in the log record', () => {
		const log = formatObserverError();

		expect(log).toBe('buzz-observer: operation failed');
	});
});
