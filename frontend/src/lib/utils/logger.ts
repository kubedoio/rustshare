/**
 * Structured logging utility for RustShare frontend.
 *
 * In production builds, debug logs are suppressed.
 * Error and warn logs are always emitted but can be extended
 * to report to a telemetry service (e.g. Sentry) in the future.
 */

const IS_DEV = import.meta.env.DEV;

function prefix(level: string): string {
	return `[rustshare:${level}]`;
}

export const logger = {
	debug(...args: unknown[]): void {
		if (IS_DEV) {
			// eslint-disable-next-line no-console
			console.log(prefix('debug'), ...args);
		}
	},

	info(...args: unknown[]): void {
		if (IS_DEV) {
			// eslint-disable-next-line no-console
			console.info(prefix('info'), ...args);
		}
	},

	warn(...args: unknown[]): void {
		// Always emit warnings — they indicate recoverable issues
		// eslint-disable-next-line no-console
		console.warn(prefix('warn'), ...args);
	},

	error(...args: unknown[]): void {
		// Always emit errors — they indicate bugs or operational issues
		// eslint-disable-next-line no-console
		console.error(prefix('error'), ...args);
	}
};
