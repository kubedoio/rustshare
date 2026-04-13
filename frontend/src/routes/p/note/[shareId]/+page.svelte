<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { renderMarkdown } from '$lib/utils/markdown';
  import { getPublicNote } from '$lib/api/notes';
  import type { PublicNoteResponse } from '$lib/api/notes';
  import { FileText, AlertCircle } from 'lucide-svelte';

  const shareId = $page.params.shareId;

  let note: PublicNoteResponse | null = null;
  let error: string | null = null;
  let isLoading = true;
  let renderedContent = '';

  onMount(() => {
    void (async () => {
      if (!shareId) {
        error = 'Invalid share link';
        isLoading = false;
        return;
      }
      try {
        note = await getPublicNote(shareId);
        renderedContent = renderMarkdown(note.content);
      } catch (err) {
        error = err instanceof Error ? err.message : 'Failed to load note';
      } finally {
        isLoading = false;
      }
    })();
  });
</script>

<svelte:head>
  <title>{note?.title || 'Shared Note'} - RustShare</title>
  <meta name="description" content={note?.excerpt || 'A shared note on RustShare'} />
</svelte:head>

<div class="public-page">
  <header class="public-header">
    <div class="public-header-inner">
      <a href="/" class="public-logo">
        <FileText size={20} class="text-brand-500" />
        <span>RustShare</span>
      </a>
    </div>
  </header>

  <main class="public-main">
    {#if isLoading}
      <div class="public-loading">
        <span class="loading loading-spinner loading-lg"></span>
        <p>Loading note...</p>
      </div>
    {:else if error}
      <div class="public-error">
        <AlertCircle size={32} />
        <h1>Note not found</h1>
        <p>This note may have been removed or made private.</p>
        <a href="/" class="btn btn-primary btn-sm mt-4">Go to RustShare</a>
      </div>
    {:else if note}
      <article class="public-note">
        <h1 class="public-note-title">{note.title}</h1>
        {#if note.excerpt && note.excerpt !== note.title}
          <p class="public-note-excerpt">{note.excerpt}</p>
        {/if}
        <div class="public-note-meta">
          <span>Updated {new Date(note.updated_at).toLocaleDateString()}</span>
        </div>
        <div class="public-note-content prose prose-sm max-w-none">
          {@html renderedContent}
        </div>
      </article>
    {/if}
  </main>

  <footer class="public-footer">
    <p>Shared with RustShare</p>
  </footer>
</div>

<style>
  .public-page {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    background: #fafafa;
  }

  .public-header {
    border-bottom: 1px solid #e5e7eb;
    background: #ffffff;
  }

  .public-header-inner {
    max-width: 720px;
    margin: 0 auto;
    padding: 1rem 1.5rem;
  }

  .public-logo {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    color: #111827;
    text-decoration: none;
  }

  .public-main {
    flex: 1;
    padding: 2rem 1rem;
  }

  .public-loading,
  .public-error {
    max-width: 720px;
    margin: 0 auto;
    text-align: center;
    padding: 3rem 1rem;
    color: #6b7280;
  }

  .public-error h1 {
    font-size: 1.25rem;
    font-weight: 600;
    color: #111827;
    margin-top: 1rem;
  }

  .public-error p {
    margin-top: 0.5rem;
  }

  .public-note {
    max-width: 720px;
    margin: 0 auto;
    background: #ffffff;
    border-radius: 0.75rem;
    padding: 2rem 2.5rem;
    box-shadow: 0 1px 3px rgba(0,0,0,0.05);
  }

  .public-note-title {
    font-size: 1.875rem;
    font-weight: 700;
    line-height: 1.2;
    color: #111827;
    margin: 0 0 0.5rem;
  }

  .public-note-excerpt {
    font-size: 1rem;
    color: #4b5563;
    margin: 0 0 1rem;
    line-height: 1.5;
  }

  .public-note-meta {
    font-size: 0.875rem;
    color: #9ca3af;
    margin-bottom: 1.5rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid #f3f4f6;
  }

  .public-note-content {
    color: #374151;
    line-height: 1.7;
  }

  .public-note-content :global(h1),
  .public-note-content :global(h2),
  .public-note-content :global(h3) {
    color: #111827;
    margin-top: 1.5rem;
    margin-bottom: 0.75rem;
  }

  .public-note-content :global(p) {
    margin-bottom: 1rem;
  }

  .public-note-content :global(ul),
  .public-note-content :global(ol) {
    margin-bottom: 1rem;
    padding-left: 1.5rem;
  }

  .public-note-content :global(li) {
    margin-bottom: 0.25rem;
  }

  .public-note-content :global(blockquote) {
    border-left: 3px solid #e5e7eb;
    padding-left: 1rem;
    color: #6b7280;
    font-style: italic;
    margin: 1rem 0;
  }

  .public-note-content :global(pre) {
    background: #f9fafb;
    padding: 1rem;
    border-radius: 0.5rem;
    overflow-x: auto;
    margin: 1rem 0;
  }

  .public-note-content :global(code) {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.875em;
    background: #f3f4f6;
    padding: 0.125rem 0.375rem;
    border-radius: 0.25rem;
  }

  .public-note-content :global(pre code) {
    background: transparent;
    padding: 0;
  }

  .public-note-content :global(a) {
    color: #2563eb;
    text-decoration: underline;
  }

  .public-footer {
    text-align: center;
    padding: 1.5rem 1rem;
    font-size: 0.875rem;
    color: #9ca3af;
  }

  @media (max-width: 640px) {
    .public-note {
      padding: 1.5rem;
      border-radius: 0;
      box-shadow: none;
    }

    .public-main {
      padding: 0;
    }
  }
</style>
