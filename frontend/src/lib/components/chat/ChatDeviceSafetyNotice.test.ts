import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import ChatDeviceSafetyNotice from './ChatDeviceSafetyNotice.svelte';

describe('Chat device safety notice', () => {
	it('distinguishes platform credential revocation from shared Buzz key revocation', () => {
		render(ChatDeviceSafetyNotice);

		expect(screen.getByRole('heading', { name: 'Chat devices and lost devices' })).toBeTruthy();
		expect(screen.getByText(/device token does not prove.*Buzz signing key/i)).toBeTruthy();
		expect(screen.getByText(/cannot erase a copied key or invalidate signatures/i)).toBeTruthy();
		expect(screen.getByText(/this page does not perform that rotation/i)).toBeTruthy();
	});
});
