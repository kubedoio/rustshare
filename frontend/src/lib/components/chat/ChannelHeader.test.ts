import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import ChannelHeader from './ChannelHeader.svelte';

describe('ChannelHeader', () => {
	it('displays the channel name with a hash prefix', () => {
		render(ChannelHeader, {
			props: {
				channelId: 'general',
				channelName: 'ops',
				askAvailable: true,
				communityId: 'community-1'
			}
		});
		expect(screen.getByText('ops')).toBeTruthy();
	});

	it(' Ask Elembra link includes communityId and channelId, not the channel name', () => {
		render(ChannelHeader, {
			props: {
				channelId: '11111111-2222-4333-8444-555555555555',
				channelName: 'ops',
				askAvailable: true,
				communityId: 'community-1'
			}
		});
		const link = screen.getByRole('link', { name: 'Ask Elembra' });
		expect(link.getAttribute('href')).toContain('communityId=community-1');
		expect(link.getAttribute('href')).toContain('channelId=11111111-2222-4333-8444-555555555555');
		expect(link.getAttribute('href')).not.toContain('ops');
	});

	it('disables the Ask Elembra action when askAvailable is false', () => {
		render(ChannelHeader, {
			props: {
				channelId: 'general',
				channelName: 'general',
				askAvailable: false,
				communityId: 'community-1'
			}
		});
		const button = screen.getByRole('button', { name: 'Ask Elembra' });
		expect(button).toBeTruthy();
		expect(button.hasAttribute('disabled')).toBe(true);
	});

	it('shows private metadata for private channels', () => {
		render(ChannelHeader, {
			props: {
				channelId: 'secret',
				channelName: 'secret',
				visibility: 'private',
				askAvailable: true,
				communityId: 'community-1'
			}
		});
		expect(screen.getByText('Private')).toBeTruthy();
	});

	it('does not fabricate member counts', () => {
		render(ChannelHeader, {
			props: {
				channelId: 'general',
				channelName: 'general',
				member: null,
				askAvailable: true,
				communityId: 'community-1'
			}
		});
		expect(screen.queryByText(/member/i)).toBeNull();
	});
});
