import { vi } from 'vitest';

// Mock browser APIs that aren't available in test environment
global.fetch = vi.fn();

// Mock localStorage
const localStorageMock = (() => {
	let store: Record<string, string> = {};

	return {
		getItem: (key: string) => store[key] || null,
		setItem: (key: string, value: string) => {
			store[key] = value.toString();
		},
		removeItem: (key: string) => {
			delete store[key];
		},
		clear: () => {
			store = {};
		}
	};
})();

Object.defineProperty(global, 'localStorage', {
	value: localStorageMock
});

// Mock window.location
Object.defineProperty(global, 'location', {
	value: {
		origin: 'http://localhost:3000',
		href: 'http://localhost:3000/',
		pathname: '/',
		search: '',
		hash: ''
	},
	writable: true
});

// Mock navigator.clipboard
Object.defineProperty(navigator, 'clipboard', {
	value: {
		writeText: vi.fn().mockResolvedValue(undefined),
		readText: vi.fn().mockResolvedValue('')
	},
	writable: true
});

// Mock Image constructor for thumbnail tests
class MockImage {
	onload: (() => void) | null = null;
	onerror: (() => void) | null = null;
	src = '';
	width = 800;
	height = 600;
	crossOrigin = '';

	constructor() {
		// Simulate image load after src is set
		setTimeout(() => {
			if (this.onload) {
				this.onload();
			}
		}, 10);
	}
}

(global as any).Image = MockImage;

// Mock Canvas API for thumbnail generation
class MockCanvasRenderingContext2D {
	canvas = { width: 0, height: 0 };
	drawImage = vi.fn();
	clearRect = vi.fn();
	fillRect = vi.fn();
	getImageData = vi.fn();
	putImageData = vi.fn();
	translate = vi.fn();
	rotate = vi.fn();
	scale = vi.fn();
	imageSmoothingEnabled = true;
	imageSmoothingQuality = 'low';
}

class MockHTMLCanvasElement {
	width = 0;
	height = 0;
	private context = new MockCanvasRenderingContext2D();

	getContext(type: string) {
		if (type === '2d') {
			return this.context;
		}
		return null;
	}

	toDataURL(type: string, quality?: number) {
		return 'data:image/jpeg;base64,mockImageData';
	}

	toBlob(callback: (blob: Blob | null) => void, type?: string, quality?: number) {
		callback(new Blob(['mock'], { type: type || 'image/png' }));
	}
}

// Override document.createElement for canvas
const originalCreateElement = document.createElement.bind(document);
document.createElement = vi.fn((tagName: string) => {
	if (tagName === 'canvas') {
		return new MockHTMLCanvasElement() as any;
	}
	return originalCreateElement(tagName);
}) as any;

// Note: console.error and console.warn are intentionally NOT mocked here.
// If a test triggers a console error, the test should fail or the error should be fixed.
// Mock console methods per-test only when testing error-handling code paths explicitly.
