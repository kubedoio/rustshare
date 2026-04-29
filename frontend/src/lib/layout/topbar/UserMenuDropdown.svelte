<script lang="ts">
	import { ChevronDown, User, Settings, Shield, LogOut, Bell, Smartphone } from 'lucide-svelte';
	import type { User as UserType } from '$lib/api/types';

	export let user: UserType | null;
	export let unreadCount: number;
	export let onLogout: () => void;
	export let open = false;

	function getInitials(name: string): string {
		return name?.charAt(0).toUpperCase() || '?';
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			open = false;
		}
	}
</script>

<svelte:window on:keydown={handleKeydown} />

{#if user}
	<div class="relative ml-1">
		<button
			type="button"
			class="flex items-center gap-2 rounded-xl border border-base-300/60 bg-base-100/50 p-1 pr-3 transition-all hover:border-brand-500/20 hover:bg-base-200"
			on:click={() => (open = !open)}
			aria-expanded={open}
			aria-haspopup="menu"
		>
			<div
				class="flex h-8 w-8 items-center justify-center rounded-xl bg-brand-500 font-bold text-white shadow-sm"
			>
				{getInitials(user.display_name)}
			</div>
			<span
				class="hidden max-w-[120px] truncate text-body-sm font-semibold text-base-content/80 md:block"
			>
				{user.display_name}
			</span>
			<ChevronDown size={14} class="opacity-40" />
		</button>

		{#if open}
			<div
				role="menu"
				class="animate-in fade-in slide-in-from-top-2 absolute right-0 mt-2 w-56 origin-top-right rounded-2xl border border-base-300 bg-base-100 py-1.5 shadow-xl ring-1 ring-black/5 duration-100"
			>
				<div class="mb-1 border-b border-base-200 px-4 py-3">
					<p class="truncate text-sm font-bold text-base-content">{user.display_name}</p>
					<p class="truncate text-meta font-medium tracking-wider text-base-content/50 uppercase">
						{user.email}
					</p>
				</div>

				<a
					href="/profile"
					role="menuitem"
					class="flex items-center gap-3 px-4 py-2 text-sm font-medium transition-colors hover:bg-base-200"
				>
					<User size={16} class="text-base-content/60" /> Profile
				</a>
				<a
					href="/notifications"
					role="menuitem"
					class="flex items-center justify-between px-4 py-2 text-sm font-medium transition-colors hover:bg-base-200"
				>
					<div class="flex items-center gap-3">
						<Bell size={16} class="text-base-content/60" /> Notifications
					</div>
					{#if unreadCount > 0}
						<span class="badge badge-sm badge-error">{unreadCount}</span>
					{/if}
				</a>
				<a
					href="/settings?tab=devices"
					role="menuitem"
					class="flex items-center gap-3 px-4 py-2 text-sm font-medium transition-colors hover:bg-base-200"
				>
					<Smartphone size={16} class="text-base-content/60" /> Devices
				</a>
				<a
					href="/settings"
					role="menuitem"
					class="flex items-center gap-3 px-4 py-2 text-sm font-medium transition-colors hover:bg-base-200"
				>
					<Settings size={16} class="text-base-content/60" /> Settings
				</a>
				{#if user.is_admin}
					<a
						href="/admin"
						role="menuitem"
						class="flex items-center gap-3 px-4 py-2 text-sm font-medium transition-colors hover:bg-base-200"
					>
						<Shield size={16} class="text-brand-500" /> Admin Panel
					</a>
				{/if}

				<div class="mt-1 border-t border-base-200 pt-1.5">
					<button
						on:click={onLogout}
						role="menuitem"
						class="flex w-full items-center gap-3 px-4 py-2 text-sm font-bold text-error transition-colors hover:bg-error/10"
					>
						<LogOut size={16} /> Sign out
					</button>
				</div>
			</div>
		{/if}
	</div>
{/if}
