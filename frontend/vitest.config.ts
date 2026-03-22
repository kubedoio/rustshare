import { defineConfig } from 'vitest/config';
import { sveltekit } from '@sveltejs/kit/vite';

export default defineConfig({
  plugins: [sveltekit()],
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    globals: true,
    environment: 'happy-dom',
    setupFiles: ['./src/test-setup.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: [
        'node_modules/',
        'src/test-setup.ts',
        '**/*.test.ts',
        '**/*.spec.ts',
        'src/routes/**/*.svelte', // E2E tests cover these
        '.svelte-kit/'
      ]
    }
  },
  resolve: {
    conditions: ['browser'],
    alias: {
      $lib: '/src/lib',
      $app: '/node_modules/@sveltejs/kit/src/runtime/app'
    }
  }
});
