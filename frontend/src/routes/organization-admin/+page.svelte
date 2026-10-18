<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	onMount(() => goto('/organization-admin/users'));
</script>

<p role="status">Opening organization users…</p>
