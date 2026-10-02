import { beforeEach, describe, it, expect, vi } from 'vitest';
import { calendarApi } from './calendar';
import { apiClient } from './client';

vi.mock('./client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn(),
		patch: vi.fn(),
		delete: vi.fn(),
		getBaseURL: vi.fn(() => 'http://localhost:8080/api/v1')
	}
}));

describe('calendarApi', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('lists events for a range with filters', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ events: [{ id: 'evt-1' }] });

		const result = await calendarApi.listEvents({
			from: '2026-10-01T00:00:00Z',
			to: '2026-11-01T00:00:00Z',
			sourceIds: ['src-1', 'src-2'],
			includeCancelled: true
		});

		expect(result).toHaveLength(1);
		expect(apiClient.get).toHaveBeenCalledWith(
			'/calendar/events?from=2026-10-01T00%3A00%3A00Z&to=2026-11-01T00%3A00%3A00Z&source_id=src-1&source_id=src-2&include_cancelled=true'
		);
	});

	it('lists events without optional filters', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ events: [] });

		await calendarApi.listEvents({ from: '2026-10-01T00:00:00Z', to: '2026-10-08T00:00:00Z' });

		expect(apiClient.get).toHaveBeenCalledWith(
			'/calendar/events?from=2026-10-01T00%3A00%3A00Z&to=2026-10-08T00%3A00%3A00Z'
		);
	});

	it('creates internal events', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({ id: 'evt-1' });
		const input = {
			title: 'Dentist',
			description: null,
			location: null,
			starts_at: '2026-10-03T08:00:00Z',
			ends_at: '2026-10-03T08:30:00Z',
			all_day: false,
			timezone: 'Europe/Berlin',
			rrule: null
		};

		await calendarApi.createEvent(input);

		expect(apiClient.post).toHaveBeenCalledWith('/calendar/events', input);
	});

	it('updates and deletes events', async () => {
		vi.mocked(apiClient.patch).mockResolvedValueOnce({ id: 'evt-1' });
		vi.mocked(apiClient.delete).mockResolvedValueOnce(undefined);

		await calendarApi.updateEvent('evt-1', { title: 'Renamed' });
		await calendarApi.deleteEvent('evt-1');

		expect(apiClient.patch).toHaveBeenCalledWith('/calendar/events/evt-1', { title: 'Renamed' });
		expect(apiClient.delete).toHaveBeenCalledWith('/calendar/events/evt-1');
	});

	it('lists sources', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ sources: [{ id: 'src-1' }] });

		const result = await calendarApi.listSources();

		expect(result).toHaveLength(1);
		expect(apiClient.get).toHaveBeenCalledWith('/calendar/sources');
	});

	it('creates ical_import sources and updates them', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({ id: 'src-1' });
		vi.mocked(apiClient.patch).mockResolvedValueOnce({ id: 'src-1' });

		await calendarApi.createIcsSource('Exported from Apple');
		await calendarApi.updateSource('src-1', { is_enabled: false });

		expect(apiClient.post).toHaveBeenCalledWith('/calendar/sources', {
			kind: 'ical_import',
			display_name: 'Exported from Apple'
		});
		expect(apiClient.patch).toHaveBeenCalledWith('/calendar/sources/src-1', { is_enabled: false });
	});

	it('deletes sources', async () => {
		vi.mocked(apiClient.delete).mockResolvedValueOnce(undefined);

		await calendarApi.deleteSource('src-1');

		expect(apiClient.delete).toHaveBeenCalledWith('/calendar/sources/src-1');
	});

	it('uploads .ics files as multipart form data with an optional target source', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({ job_id: 'job-1', source_id: 'src-1' });
		const file = new File(['BEGIN:VCALENDAR'], 'export.ics', { type: 'text/calendar' });

		await calendarApi.uploadIcs(file, 'src-1');

		expect(apiClient.post).toHaveBeenCalledWith('/calendar/import', expect.any(FormData));
		const form = vi.mocked(apiClient.post).mock.calls[0][1] as FormData;
		expect(form.get('file')).toBe(file);
		expect(form.get('source_id')).toBe('src-1');
	});

	it('omits source_id when uploading without a target source', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({ job_id: 'job-1', source_id: 'src-2' });
		const file = new File(['BEGIN:VCALENDAR'], 'export.ics');

		await calendarApi.uploadIcs(file);

		const form = vi.mocked(apiClient.post).mock.calls[0][1] as FormData;
		expect(form.get('source_id')).toBeNull();
	});

	it('lists and fetches import jobs', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({ jobs: [{ id: 'job-1' }] });
		vi.mocked(apiClient.get).mockResolvedValueOnce({ id: 'job-1', status: 'completed' });

		const jobs = await calendarApi.listImportJobs();
		const job = await calendarApi.getImportJob('job-1');

		expect(jobs).toHaveLength(1);
		expect(job.status).toBe('completed');
		expect(apiClient.get).toHaveBeenNthCalledWith(1, '/calendar/import-jobs');
		expect(apiClient.get).toHaveBeenNthCalledWith(2, '/calendar/import-jobs/job-1');
	});

	it('connectSource requests the provider connect endpoint and returns the authorize URL', async () => {
		vi.mocked(apiClient.get).mockResolvedValueOnce({
			authorize_url: 'https://accounts.google.com/o/oauth2/v2/auth?client_id=x&state=y'
		});

		const result = await calendarApi.connectSource('google');

		expect(apiClient.get).toHaveBeenCalledWith('/calendar/sources/google/connect');
		expect(result.authorize_url).toContain('accounts.google.com');
	});

	it('disconnectSource posts to the source disconnect endpoint', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({});

		await calendarApi.disconnectSource('src-1');

		expect(apiClient.post).toHaveBeenCalledWith('/calendar/sources/src-1/disconnect');
	});

	it('resyncSource posts to the resync endpoint and propagates a 409', async () => {
		vi.mocked(apiClient.post).mockResolvedValueOnce({});

		await calendarApi.resyncSource('src-1');

		expect(apiClient.post).toHaveBeenCalledWith('/calendar/sources/src-1/resync');

		vi.mocked(apiClient.post).mockRejectedValueOnce(
			Object.assign(new Error('sync in progress'), { status: 409 })
		);

		await expect(calendarApi.resyncSource('src-1')).rejects.toMatchObject({ status: 409 });
	});
});
