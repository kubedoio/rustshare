
## 2024-05-24 - [Fix XSS vulnerability in Markdown rendering]
**Vulnerability:** A Cross-Site Scripting (XSS) vulnerability was identified in the custom `renderMarkdown` utility in `frontend/src/lib/utils/markdown.ts`. Simplistic markdown parsers that turn `[text](url)` into `<a href="url">` are vulnerable to `javascript:` URIs.
**Learning:** SvelteKit SSR environments require `isomorphic-dompurify` instead of standard `dompurify` to sanitize HTML output on the server-side gracefully. Standard `dompurify` throws errors without a `window` object unless manually configured with `jsdom`, which is heavy.
**Prevention:** Always use a robust HTML sanitizer library like `isomorphic-dompurify` when rendering user-generated Markdown into raw HTML, especially to guard against malicious protocol schemes (`javascript:`, `data:`) and inline event handlers.
