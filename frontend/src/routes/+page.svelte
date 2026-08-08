<script lang="ts">
	import { goto } from '$app/navigation';
	import { authStore } from '$lib/stores/auth';

	// Wait for the auth session bootstrap to finish before redirecting, otherwise
	// an authenticated user can be bounced to /login while the session is still loading.
	let redirected = $state(false);

	$effect(() => {
		if (redirected || $authStore.isLoading) return;
		redirected = true;
		goto($authStore.isAuthenticated ? '/dashboard' : '/login');
	});
</script>

<!-- Show nothing, just redirect -->
<div class="flex min-h-screen items-center justify-center">
	<div class="loading loading-lg loading-spinner"></div>
</div>
