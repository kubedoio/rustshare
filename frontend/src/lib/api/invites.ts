import { apiClient } from './client';
import type { User } from './types';

export interface CreateInviteRequest {
	recipient_email: string;
	origin: string;
}

export interface CreateInviteResponse {
	token: string;
	invite_link: string;
	expires_at: string;
}

export interface InviteDetail {
	sender_name: string;
	recipient_email: string;
	subject: string;
	body: string;
	terms_enabled: boolean;
	terms_text?: string;
	expires_at: string;
}

export interface AcceptInviteRequest {
	display_name: string;
	email: string;
	password: string;
	terms_accepted?: boolean;
}

export const createInvite = (data: CreateInviteRequest) =>
	apiClient.post<CreateInviteResponse>('/invites', data);
export const getInvite = (token: string) =>
	apiClient.get<InviteDetail>(`/invites/${token}`);
export const acceptInvite = (token: string, data: AcceptInviteRequest) =>
	apiClient.post<User>(`/invites/${token}/accept`, data);
