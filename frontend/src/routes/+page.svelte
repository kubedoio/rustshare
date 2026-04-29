<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { currentUser } from '$lib/stores/auth';
	import { get } from 'svelte/store';

	onMount(() => {
		const user = get(currentUser);
		if (user) {
			goto('/dashboard');
		} else {
			goto('/login');
		}
	});
</script>

<!-- Show nothing, just redirect -->
<div class="flex min-h-screen items-center justify-center">
	<div class="loading loading-lg loading-spinner"></div>
</div>
