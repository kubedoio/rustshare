import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { readable } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Page from './+page.svelte';
import { queryClient } from '$lib/query-client';
import { ApiError } from '$lib/api/types';

const mocks = vi.hoisted(() => ({
	getChatStatus: vi.fn(),
	getChatCommunityMapping: vi.fn(),
	provisionChatCommunity: vi.fn(),
	connectChatCommunity: vi.fn()
}));

vi.mock('$lib/api/chat', () => ({
	getChatStatus: mocks.getChatStatus,
	getChatCommunityMapping: mocks.getChatCommunityMapping,
	provisionChatCommunity: mocks.provisionChatCommunity,
	connectChatCommunity: mocks.connectChatCommunity
}));

vi.mock('$lib/stores/auth', () => ({
	currentUser: readable({ tenant_id: 'workspace-1' })
}));

describe('admin chat settings page', () => {
	beforeEach(() => {
		vi.mocked(mocks.getChatStatus).mockReset();
		vi.mocked(mocks.getChatCommunityMapping).mockReset();
		vi.mocked(mocks.provisionChatCommunity).mockReset();
		vi.mocked(mocks.connectChatCommunity).mockReset();
		mocks.getChatStatus.mockResolvedValue({
			chat_enabled: true,
			mapping: null,
			binding: null,
			admission_active: false,
			ask_available: false
		});
		mocks.getChatCommunityMapping.mockRejectedValue(new ApiError(404, 'not found'));
		queryClient.clear();
	});

	it('renders the setup action and the unconfigured state', async () => {
		render(Page);
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Set up automatically' })).toBeTruthy()
		);
		await waitFor(() =>
			expect(screen.getByText('Chat is not yet connected to a community.')).toBeTruthy()
		);
	});

	it('shows mapping details when a mapping exists', async () => {
		mocks.getChatCommunityMapping.mockResolvedValue({
			community_id: 'community-1',
			relay_url: 'wss://relay.example',
			relay_pubkey: 'pk-1',
			active: true
		});
		render(Page);
		await waitFor(() => expect(screen.getByText('community-1')).toBeTruthy());
		expect(screen.getByText('wss://relay.example')).toBeTruthy();
		expect(screen.getByText('pk-1')).toBeTruthy();
	});

	it('provisions on click and shows the returned community id', async () => {
		mocks.provisionChatCommunity.mockResolvedValue({
			status: 'created',
			community_id: 'community-9',
			relay_url: 'wss://relay.example',
			relay_pubkey: 'pk-9'
		});
		render(Page);
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Set up automatically' })).toBeTruthy()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Set up automatically' }));
		await waitFor(() => expect(mocks.provisionChatCommunity).toHaveBeenCalledWith('workspace-1'));
		await waitFor(() =>
			expect(screen.getByText(/Connected to community community-9 \(created\)/)).toBeTruthy()
		);
	});

	it('connects an existing deployment from the form', async () => {
		render(Page);
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Connect existing Chat deployment' })).toBeTruthy()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Connect existing Chat deployment' }));
		await fireEvent.input(screen.getByPlaceholderText('wss://relay.example'), {
			target: { value: 'wss://relay.example' }
		});
		await fireEvent.input(screen.getByPlaceholderText(/00000000-0000-0000-0000-000000000000/), {
			target: { value: 'community-1' }
		});
		await fireEvent.click(screen.getByRole('button', { name: 'Connect' }));
		await waitFor(() =>
			expect(mocks.connectChatCommunity).toHaveBeenCalledWith('workspace-1', {
				community_id: 'community-1',
				relay_url: 'wss://relay.example'
			})
		);
	});

	it('renders the server error message when provisioning fails', async () => {
		mocks.provisionChatCommunity.mockRejectedValue(
			new ApiError(409, 'community is already mapped to another workspace')
		);
		render(Page);
		await waitFor(() =>
			expect(screen.getByRole('button', { name: 'Set up automatically' })).toBeTruthy()
		);
		await fireEvent.click(screen.getByRole('button', { name: 'Set up automatically' }));
		await waitFor(() =>
			expect(screen.getByText('community is already mapped to another workspace')).toBeTruthy()
		);
	});
});
