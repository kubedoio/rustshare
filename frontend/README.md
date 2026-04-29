# RustShare Frontend

This is the RustShare web frontend: a [SvelteKit](https://svelte.dev/docs/kit) + [Vite](https://vitejs.dev) application that provides the file management UI, sharing flows, and real-time sync interface.

## Prerequisites

- **Node.js** 22+
- **npm** (bundled with Node)

## Available Scripts

| Command              | Description                                  |
| -------------------- | -------------------------------------------- |
| `npm run dev`        | Start the development server with hot reload |
| `npm run build`      | Create an optimized production build         |
| `npm run preview`    | Preview the production build locally         |
| `npm run test`       | Run the Vitest unit test suite               |
| `npm run test:watch` | Run Vitest in watch mode                     |
| `npm run test:e2e`   | Run Playwright end-to-end tests              |
| `npm run check`      | Run TypeScript and Svelte type checking      |
| `npm run lint`       | Run Prettier and ESLint                      |
| `npm run format`     | Auto-format code with Prettier               |

## Environment Variables

The frontend reads build-time environment variables prefixed with `VITE_`:

| Variable       | Default   | Description                           |
| -------------- | --------- | ------------------------------------- |
| `VITE_API_URL` | `/api/v1` | Base path for REST API calls          |
| `VITE_WS_URL`  | `/api/ws` | WebSocket endpoint for real-time sync |

These are baked into the build at compile time. If you change them, you must rebuild.

For the Docker Compose setup, the backend Dockerfile sets the correct defaults. For local development against a running backend:

```bash
VITE_API_URL=http://localhost:8080/api/v1 \
VITE_WS_URL=ws://localhost:8080/api/ws \
  npm run dev
```

## Development

For the full contributor setup—including how to run the backend, database, and object storage locally—see [docs/development.md](../docs/development.md).
