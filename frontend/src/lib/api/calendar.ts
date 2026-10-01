import { apiClient } from './client';

export type CalendarSourceKind = 'internal' | 'ical_import' | 'google' | 'outlook';

export type CalendarSourceStatus =
	'healthy' | 'degraded' | 'auth_required' | 'rate_limited' | 'paused' | 'failed';

export type CalendarEventStatus = 'confirmed' | 'cancelled' | 'tentative';

export interface CalendarEvent {
	id: string;
	source_id: string;
	source_kind: CalendarSourceKind;
	title: string;
	description: string | null;
	location: string | null;
	starts_at: string;
	ends_at: string;
	all_day: boolean;
	original_date: string | null;
	timezone: string | null;
	rrule: string | null;
	recurrence_id: string | null;
	instance_start: string | null;
	status: CalendarEventStatus;
	read_only: boolean;
	created_at: string;
	updated_at: string;
}

export interface CalendarSource {
	id: string;
	kind: CalendarSourceKind;
	display_name: string;
	external_account: string | null;
	external_calendar_id: string | null;
	is_enabled: boolean;
	status: CalendarSourceStatus;
	last_synced_at: string | null;
	last_error: string | null;
	created_at: string;
}

export type CalendarImportJobStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface CalendarImportJob {
	id: string;
	source_id: string;
	filename: string;
	status: CalendarImportJobStatus;
	total_events: number;
	processed_events: number;
	failed_events: number;
	last_error: string | null;
	started_at: string | null;
	completed_at: string | null;
	created_at: string;
}

export interface CreateCalendarEventRequest {
	title: string;
	description?: string | null;
	location?: string | null;
	starts_at: string;
	ends_at: string;
	all_day?: boolean;
	timezone?: string | null;
	rrule?: string | null;
}

export type UpdateCalendarEventRequest = Partial<CreateCalendarEventRequest>;

export interface ListCalendarEventsParams {
	from: string;
	to: string;
	sourceIds?: string[];
	includeCancelled?: boolean;
}

export interface CalendarImportResponse {
	job_id: string;
	source_id: string;
	status: CalendarImportJobStatus;
}

export const calendarApi = {
	listEvents: async (params: ListCalendarEventsParams): Promise<CalendarEvent[]> => {
		const query = new URLSearchParams({ from: params.from, to: params.to });
		for (const sourceId of params.sourceIds ?? []) {
			query.append('source_id', sourceId);
		}
		if (params.includeCancelled) query.set('include_cancelled', 'true');
		const res = await apiClient.get<{ events: CalendarEvent[] }>(`/calendar/events?${query}`);
		return res.events;
	},

	createEvent: async (input: CreateCalendarEventRequest): Promise<CalendarEvent> => {
		return apiClient.post<CalendarEvent>('/calendar/events', input);
	},

	updateEvent: async (
		eventId: string,
		input: UpdateCalendarEventRequest
	): Promise<CalendarEvent> => {
		return apiClient.patch<CalendarEvent>(`/calendar/events/${eventId}`, input);
	},

	deleteEvent: async (eventId: string): Promise<void> => {
		await apiClient.delete(`/calendar/events/${eventId}`);
	},

	listSources: async (): Promise<CalendarSource[]> => {
		const res = await apiClient.get<{ sources: CalendarSource[] }>('/calendar/sources');
		return res.sources;
	},

	createIcsSource: async (displayName: string): Promise<CalendarSource> => {
		return apiClient.post<CalendarSource>('/calendar/sources', {
			kind: 'ical_import',
			display_name: displayName
		});
	},

	updateSource: async (
		sourceId: string,
		input: { display_name?: string; is_enabled?: boolean }
	): Promise<CalendarSource> => {
		return apiClient.patch<CalendarSource>(`/calendar/sources/${sourceId}`, input);
	},

	deleteSource: async (sourceId: string): Promise<void> => {
		await apiClient.delete(`/calendar/sources/${sourceId}`);
	},

	uploadIcs: async (file: File, sourceId?: string): Promise<CalendarImportResponse> => {
		const form = new FormData();
		form.append('file', file);
		if (sourceId) form.append('source_id', sourceId);
		return apiClient.post<CalendarImportResponse>('/calendar/import', form);
	},

	listImportJobs: async (): Promise<CalendarImportJob[]> => {
		const res = await apiClient.get<{ jobs: CalendarImportJob[] }>('/calendar/import-jobs');
		return res.jobs;
	},

	getImportJob: async (jobId: string): Promise<CalendarImportJob> => {
		return apiClient.get<CalendarImportJob>(`/calendar/import-jobs/${jobId}`);
	}
};
