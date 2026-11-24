<script lang="ts">
	import { goto } from '$app/navigation';
	import { ApiError } from '$lib/api/types';
	import {
		askWorkspace,
		openAskCitation,
		searchMemory,
		type AskCitation,
		type OpenCitationResponse,
		type AskResponse,
		type AskScope,
		type MemorySearchResult
	} from '$lib/api/ask';
	import { currentUser } from '$lib/stores/auth';
	import { ArrowUp, BookOpen, ExternalLink, LoaderCircle } from 'lucide-svelte';

	interface Props {
		scope?: AskScope;
		scopeLabel?: string;
		heading?: string;
		onChatCitationOpen?: (citation: OpenCitationResponse) => void;
	}

	let {
		scope = { type: 'workspace' },
		scopeLabel = 'Workspace',
		heading = 'Ask Elembra',
		onChatCitationOpen = () => {}
	}: Props = $props();
	let question = $state('');
	let response = $state<AskResponse | null>(null);
	let searchResults = $state<MemorySearchResult[] | null>(null);
	let error = $state('');
	let loading = $state(false);
	let searching = $state(false);
	let openingCitation = $state<string | null>(null);
	let requestGeneration = 0;
	let scopeKey = $state('');
	const maxKeywordQueryChars = 1_000;
	let keywordQueryTooLong = $derived(Array.from(question.trim()).length > maxKeywordQueryChars);

	$effect(() => {
		const nextScopeKey = JSON.stringify(scope);
		if (nextScopeKey === scopeKey) return;
		scopeKey = nextScopeKey;
		requestGeneration += 1;
		response = null;
		searchResults = null;
		error = '';
		// A stale in-flight request skips its own finally (generation mismatch),
		// so reset these here or the form stays stuck in loading.
		loading = false;
		searching = false;
		openingCitation = null;
	});

	async function search() {
		const trimmed = question.trim();
		if (!trimmed || searching || loading || scope.type !== 'workspace') return;
		if (Array.from(trimmed).length > maxKeywordQueryChars) {
			error = `Keyword search supports up to ${maxKeywordQueryChars} characters. Shorten your query to search.`;
			return;
		}
		const generation = ++requestGeneration;
		searching = true;
		error = '';
		searchResults = null;
		response = null;
		try {
			const results = await searchMemory(trimmed);
			if (generation === requestGeneration) searchResults = results.results;
		} catch {
			if (generation === requestGeneration) {
				error = 'Memory search could not be completed. Please try again.';
			}
		} finally {
			if (generation === requestGeneration) searching = false;
		}
	}

	function errorMessage(value: unknown): string {
		if (value instanceof ApiError) {
			if (value.status === 429) return 'Ask is busy right now. Please try again shortly.';
			if (value.status === 503)
				return 'Ask is temporarily unavailable because the language model is not configured.';
			if (value.status === 401 || value.status === 403 || value.status === 404)
				return 'This Ask scope is not available.';
		}
		return 'Ask could not be completed. Please try again.';
	}

	async function submit() {
		const trimmed = question.trim();
		if (!trimmed || loading || !$currentUser?.tenant_id) return;
		const generation = ++requestGeneration;
		loading = true;
		error = '';
		response = null;
		searchResults = null;
		try {
			const result = await askWorkspace({
				question: trimmed,
				workspace_id: $currentUser.tenant_id,
				scope,
				result_limit: 8
			});
			if (generation === requestGeneration) response = result;
		} catch (value) {
			if (generation === requestGeneration) error = errorMessage(value);
		} finally {
			if (generation === requestGeneration) loading = false;
		}
	}

	function fileId(resourceRef: string, type: 'file' | 'folder'): string | null {
		const prefix = `elembra://io.elembra.files/${type}/`;
		const ref = resourceRef.split(/[?#]/, 1)[0];
		return ref.startsWith(prefix) ? ref.slice(prefix.length) : null;
	}

	function mailMessageId(resourceRef: string): string | null {
		const prefix = 'elembra://io.elembra.mail/message/';
		const ref = resourceRef.split(/[?#]/, 1)[0];
		return ref.startsWith(prefix) ? ref.slice(prefix.length) : null;
	}

	function sourceVersion(resourceRef: string): string | null {
		try {
			return new URL(resourceRef).searchParams.get('version');
		} catch {
			return null;
		}
	}

	function sourceApplication(citation: AskCitation): string {
		if (citation.provenance.community_id || citation.provenance.channel_id) return 'Chat';
		if (citation.resource_ref.startsWith('elembra://io.elembra.mail/message/')) return 'Mail';
		if (citation.provenance.note_id) return 'Files · Note';
		return citation.provenance.file_id ? 'Files' : 'Authorized source';
	}

	async function openCitation(citation: AskCitation) {
		openingCitation = citation.resource_ref;
		try {
			const opened = await openAskCitation(citation.resource_ref);
			if (!opened.available) {
				error = 'That source is no longer available.';
				return;
			}
			const file = fileId(opened.resource_ref, 'file');
			const folder = fileId(opened.resource_ref, 'folder');
			const mailMessage = mailMessageId(opened.resource_ref);
			const version = sourceVersion(opened.resource_ref);
			const noteId = citation.provenance.note_id;
			if (file && noteId === file) {
				if (!version) {
					error = 'The cited Note version is unavailable.';
					return;
				}
				goto(
					`/apps/notes/${encodeURIComponent(noteId)}?sourceVersion=${encodeURIComponent(version)}`
				);
			} else if (file) {
				const versionParam = version ? `&version=${encodeURIComponent(version)}` : '';
				goto(`/files?preview=${encodeURIComponent(file)}${versionParam}`);
			} else if (folder) goto(`/files?folder=${encodeURIComponent(folder)}`);
			else if (mailMessage) goto(`/apps/mail/messages/${encodeURIComponent(mailMessage)}`);
			else if (opened.resource_ref.startsWith('elembra://io.elembra.chat/')) {
				onChatCitationOpen(opened);
			}
		} catch {
			error = 'That source is no longer available.';
		} finally {
			openingCitation = null;
		}
	}
</script>

<section class="mx-auto w-full max-w-4xl" aria-labelledby="ask-heading">
	<div class="mb-8">
		<p class="mb-2 text-sm font-medium text-brand-600">Company Memory · {scopeLabel}</p>
		<h1 id="ask-heading" class="text-3xl font-semibold tracking-tight text-base-content">
			{heading}
		</h1>
		<p class="mt-2 max-w-2xl text-base text-base-content/60">
			Ask a question about the sources you can access. Answers are grounded in the cited evidence
			below.
		</p>
	</div>

	<form
		class="rounded-2xl border border-base-300/70 bg-base-100 p-3 shadow-sm"
		onsubmit={(event) => {
			event.preventDefault();
			void submit();
		}}
	>
		<label class="sr-only" for="ask-question">Your question</label>
		<textarea
			id="ask-question"
			bind:value={question}
			maxlength="4000"
			rows="4"
			placeholder="What would you like to understand?"
			class="w-full resize-none border-0 bg-transparent p-2 text-base outline-none placeholder:text-base-content/40"
			disabled={loading || searching}></textarea>
		{#if scope.type === 'workspace' && keywordQueryTooLong}
			<p class="px-2 pb-2 text-sm text-base-content/60" aria-live="polite">
				Keyword search supports up to {maxKeywordQueryChars} characters. Shorten your query to search.
			</p>
		{/if}
		<div class="flex items-center justify-between gap-3 border-t border-base-200 pt-3">
			<span class="text-xs text-base-content/50">{question.length}/4000 · Scope: {scopeLabel}</span>
			<div class="flex gap-2">
				{#if scope.type === 'workspace'}
					<button
						type="button"
						class="btn btn-ghost btn-sm"
						disabled={loading || searching || !question.trim() || keywordQueryTooLong}
						onclick={() => void search()}
					>
						{searching ? 'Searching' : 'Search by keyword'}
					</button>
				{/if}
				<button
					type="submit"
					class="btn gap-2 btn-primary btn-sm"
					disabled={loading || searching || !question.trim() || !$currentUser?.tenant_id}
				>
					{#if loading}<LoaderCircle size={15} class="animate-spin" />{:else}<ArrowUp
							size={15}
						/>{/if}
					{loading ? 'Generating' : 'Ask'}
				</button>
			</div>
		</div>
	</form>

	{#if error}<div class="mt-5 alert alert-warning" role="alert"><span>{error}</span></div>{/if}
	{#if searchResults}
		<section class="mt-8" aria-label="Memory keyword search results" aria-live="polite">
			<h2 class="mb-3 text-sm font-semibold text-base-content">
				Search results ({searchResults.length})
			</h2>
			{#if searchResults.length}
				<div class="grid gap-2 sm:grid-cols-2">
					{#each searchResults as result (result.resource_ref)}
						<button
							type="button"
							class="w-full rounded-xl border border-base-300/70 p-3 text-left hover:bg-base-200/50"
							onclick={() => void openCitation(result)}
						>
							<h3 class="text-sm font-medium">{result.title}</h3>
							<p class="mt-1 text-xs text-base-content/50">
								{result.source_application} · {result.source_type}{#if result.location}
									· {result.location}{/if}
							</p>
							{#if result.snippet}<p class="mt-2 text-sm text-base-content/70">
									{result.snippet}
								</p>{/if}
						</button>
					{/each}
				</div>
			{:else}
				<p class="text-sm text-base-content/60">No matching authorized sources.</p>
			{/if}
		</section>
	{/if}
	{#if response}
		<article class="mt-8" aria-live="polite">
			<div class="mb-4 flex items-center gap-2 text-sm font-medium text-base-content/60">
				<BookOpen size={16} />
				{response.grounded
					? 'Grounded answer'
					: response.insufficient_evidence
						? 'Insufficient evidence'
						: 'Answer not grounded'}
			</div>
			<p class="text-lg leading-8 whitespace-pre-wrap text-base-content">{response.answer}</p>
			{#if response.citations.length}
				<div class="mt-8 border-t border-base-300/70 pt-5">
					<h2 class="mb-3 text-sm font-semibold text-base-content">
						Sources ({response.source_count})
					</h2>
					<div class="grid gap-2 sm:grid-cols-2">
						{#each response.citations as citation}
							<button
								type="button"
								class="flex items-start gap-3 rounded-xl border border-base-300/70 p-3 text-left transition-colors hover:border-brand-500/50 hover:bg-base-200/50"
								onclick={() => void openCitation(citation)}
								disabled={openingCitation === citation.resource_ref}
							>
								<ExternalLink size={16} class="mt-0.5 shrink-0 text-brand-600" />
								<span class="min-w-0"
									><span class="block truncate text-sm font-medium">{citation.title}</span><span
										class="block truncate text-xs text-base-content/50"
										>{sourceApplication(citation)} · {citation.location ||
											citation.resource_ref}</span
									></span
								>
							</button>
						{/each}
					</div>
				</div>
			{/if}
		</article>
	{/if}
</section>
