<script lang="ts">
	import { productBranding } from '$lib/branding';
	import OidcConfigForm from '$lib/components/admin/OidcConfigForm.svelte';
</script>

<svelte:head>
	<title>OIDC / SSO — Admin | {productBranding.productName}</title>
</svelte:head>

<div class="space-y-4">
	<h2 class="text-2xl font-bold">OIDC / SSO</h2>
	<OidcConfigForm />
</div>
