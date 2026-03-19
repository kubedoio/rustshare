import { describe, it, expect } from 'vitest';
import { GET } from './+server.js';

describe('Health Check Endpoint', () => {
  it('should return healthy status', async () => {
    const response = await GET({} as any);
    const data = await response.json();

    expect(response.status).toBe(200);
    expect(data.status).toBe('healthy');
    expect(data.timestamp).toBeTruthy();
    expect(data.version).toBe('0.1.0');
  });

  it('should return valid ISO timestamp', async () => {
    const response = await GET({} as any);
    const data = await response.json();

    const timestamp = new Date(data.timestamp);
    expect(timestamp.toString()).not.toBe('Invalid Date');
  });

  it('should have required fields', async () => {
    const response = await GET({} as any);
    const data = await response.json();

    expect(data).toHaveProperty('status');
    expect(data).toHaveProperty('timestamp');
    expect(data).toHaveProperty('version');
  });
});
