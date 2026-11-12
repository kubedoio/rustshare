import { describe, expect, it } from 'vitest';
import { formatObserverError } from '../../../scripts/observer-log.mjs';

describe('observer error logs', () => {
	it('does not include hostile remote message or error text in the log record', () => {
		const message = 'relay\nforged log entry';
		const error = new Error('private payload and token');
		const log = formatObserverError(message, error);

		expect(log).toBe('buzz-observer: operation failed');
		expect(log).not.toContain(message);
		expect(log).not.toContain(error.message);
	});
});
