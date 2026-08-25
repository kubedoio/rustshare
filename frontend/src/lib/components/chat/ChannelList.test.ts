import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ChannelList from './ChannelList.svelte';
import type { ChatChannelInfo } from '$lib/api/chat';

function channel(overrides: Partial<ChatChannelInfo> = {}): ChatChannelInfo {
	return {
		channel_id: 'general',
		name: 'general',
		channel_kind: 'topic',
		channel_type: null,
		visibility: null,
		member: null,
		latest_event_at: '2026-08-12T10:00:00Z',
		...overrides
	};
}

function renderList(props: {
	channels?: ChatChannelInfo[];
	loading?: boolean;
	selectedChannelId?: string | null;
	onSelect?: (id: string) => void;
}) {
	return render(ChannelList, {
		props: {
			channels: props.channels ?? [],
			loading: props.loading ?? false,
			selectedChannelId: props.selectedChannelId ?? null,
			onSelect: props.onSelect ?? vi.fn()
		}
	});
}

describe('ChannelList', () => {
	it('displays channel names, not raw ids, when a name is present', () => {
		renderList({
			channels: [channel({ channel_id: '11111111-2222-4333-8444-555555555555', name: 'ops' })]
		});
		expect(screen.getByText('ops')).toBeTruthy();
		expect(screen.queryByText('11111111-2222-4333-8444-555555555555')).toBeNull();
	});

	it('falls back to the channel id when no name is provided', () => {
		renderList({
			channels: [channel({ channel_id: 'general', name: null })]
		});
		expect(screen.getByText('general')).toBeTruthy();
	});

	it('marks the selected channel with semantic highlight classes', () => {
		renderList({
			channels: [
				channel({ channel_id: 'general', name: 'general' }),
				channel({ channel_id: 'random', name: 'random' })
			],
			selectedChannelId: 'random'
		});
		const selected = screen.getByRole('option', { selected: true });
		expect(selected.textContent).toContain('random');
		expect(selected.className).toContain('bg-base-200');
		expect(selected.className).toContain('text-primary');
	});

	it('truncates long channel names and keeps the full name in the title', () => {
		const longName = 'a-very-long-channel-name-that-needs-truncation';
		renderList({
			channels: [channel({ channel_id: 'long', name: longName })]
		});
		const button = screen.getByTitle(longName);
		expect(button).toBeTruthy();
		expect(button.querySelector('.truncate')).toBeTruthy();
	});

	it('shows a private lock indicator for private channels', () => {
		renderList({
			channels: [channel({ channel_id: 'secret', name: 'secret', visibility: 'private' })]
		});
		const option = screen.getByRole('option', { name: /secret/ });
		expect(option.querySelector('[aria-label="Private channel"]')).toBeTruthy();
	});

	it('does not show a lock for public or unnamed-visibility channels', () => {
		renderList({
			channels: [channel({ channel_id: 'open', name: 'open', visibility: 'public' })]
		});
		const option = screen.getByRole('option', { name: /open/ });
		expect(option.querySelector('[aria-label="Private channel"]')).toBeNull();
	});

	it('calls onSelect with the channel id when clicked', async () => {
		const onSelect = vi.fn();
		renderList({
			channels: [
				channel({ channel_id: 'general', name: 'general' }),
				channel({ channel_id: 'random', name: 'random' })
			],
			onSelect
		});
		await fireEvent.click(screen.getByRole('option', { name: /random/ }));
		expect(onSelect).toHaveBeenCalledWith('random');
	});

	it('renders a skeleton list while loading', () => {
		renderList({ loading: true, channels: [] });
		expect(screen.getByLabelText('Loading channels')).toBeTruthy();
	});
});
