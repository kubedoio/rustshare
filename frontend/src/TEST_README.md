# RustShare Frontend Test Suite

This directory contains comprehensive unit and integration tests for the RustShare frontend.

## Test Framework

- **Vitest**: Fast unit test runner
- **Happy DOM**: Lightweight DOM implementation for testing
- **Playwright**: E2E testing (separate configuration)

## Running Tests

```bash
# Run all tests once
npm test

# Run tests in watch mode (auto-rerun on changes)
npm run test:watch

# Run E2E tests
npm run test:e2e

# Run with coverage
npm test -- --coverage
```

## Test Structure

### Unit Tests

Tests for individual functions, utilities, and stores:

- `src/lib/stores/*.test.ts` - Store logic tests
- `src/lib/api/*.test.ts` - API client tests
- `src/lib/utils/*.test.ts` - Utility function tests

### Integration Tests

Tests that verify multiple components work together:

- `src/lib/utils/sorting.test.ts` - File/folder sorting scenarios

### E2E Tests

End-to-end tests using Playwright (in `tests/` directory):

- Full user workflows
- Cross-browser testing
- Mobile responsive verification

## Test Coverage

Current test coverage for key features:

### File Sort Store (`fileSort.test.ts`)

- ✅ Default state initialization
- ✅ Sort field selection (name, date, size, type)
- ✅ Order toggling (asc/desc)
- ✅ View mode switching (grid/list)
- ✅ localStorage persistence
- ✅ Corrupted data handling

### Selection Store (`selection.test.ts`)

- ✅ File selection/deselection
- ✅ Folder selection/deselection
- ✅ Select all functionality
- ✅ Deselect all functionality
- ✅ Selection count tracking
- ✅ Mixed file/folder selection
- ✅ Derived stores (count, hasSelection)

### Format Utilities (`format.test.ts`)

- ✅ File size formatting (bytes → TB)
- ✅ Date/time formatting
- ✅ MIME type icon mapping
- ✅ Edge cases (0 bytes, unknown types)

### Shares API (`shares.test.ts`)

- ✅ Create share with password/expiry
- ✅ List file shares
- ✅ Revoke share
- ✅ Share URL generation
- ✅ Error handling
- ✅ Permission types (View/ReadWrite)

### Sorting Logic (`sorting.test.ts`)

- ✅ Sort by name (asc/desc, case-insensitive)
- ✅ Sort by date (oldest/newest first)
- ✅ Sort by size (smallest/largest first)
- ✅ Sort by MIME type
- ✅ Folder/file separation
- ✅ Edge cases (duplicates, special characters)

## Writing Tests

### Test File Naming

- Unit tests: `*.test.ts`
- E2E tests: `*.spec.ts` (in `tests/` directory)

### Example Test

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { myFunction } from './myFunction';

describe('myFunction', () => {
	beforeEach(() => {
		// Reset state before each test
	});

	it('should do something', () => {
		const result = myFunction('input');
		expect(result).toBe('expected output');
	});

	it('should handle edge cases', () => {
		expect(myFunction('')).toBe('');
		expect(myFunction(null)).toBeNull();
	});
});
```

### Mocking

API calls and external dependencies are mocked:

```typescript
import { vi } from 'vitest';

vi.mock('$lib/api/client', () => ({
	apiClient: {
		get: vi.fn(),
		post: vi.fn()
	}
}));
```

## Test Setup

Global test configuration in `src/test-setup.ts`:

- Mock localStorage
- Mock window.location
- Mock navigator.clipboard
- Mock fetch API
- Suppress console noise

## Continuous Integration

Tests run automatically on:

- Pre-commit (via Git hooks)
- Pull requests
- Main branch pushes
- Production deployments

## Coverage Goals

Target coverage thresholds:

- **Stores**: 90%+ (critical business logic)
- **Utilities**: 80%+ (well-defined inputs/outputs)
- **API clients**: 70%+ (many integration points)
- **Components**: E2E tests (Playwright)

## Debugging Tests

```bash
# Run specific test file
npm test -- fileSort.test.ts

# Run tests matching pattern
npm test -- --grep "sort by name"

# Debug mode
npm test -- --inspect-brk

# UI mode for interactive debugging
npx vitest --ui
```

## Known Limitations

- **Svelte component tests**: Currently using E2E tests. Consider adding `@testing-library/svelte` for component unit tests.
- **WebSocket tests**: Mocked in unit tests, verified in E2E.
- **Browser-specific APIs**: Some features only testable in E2E (clipboard, drag-drop).

## Future Improvements

- [ ] Add component unit tests with Testing Library
- [ ] Increase API client coverage
- [ ] Add visual regression tests
- [ ] Performance benchmarks
- [ ] Mutation testing for test quality

## References

- [Vitest Documentation](https://vitest.dev/)
- [Happy DOM](https://github.com/capricorn86/happy-dom)
- [Playwright](https://playwright.dev/)
