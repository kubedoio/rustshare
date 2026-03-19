import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = () => {
  return json({
    status: 'healthy',
    timestamp: new Date().toISOString(),
    version: '0.1.0'
  });
};
