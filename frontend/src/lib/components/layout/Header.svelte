<script lang="ts">
  import { currentUser } from '$lib/stores/auth';

  export let onMenuClick: () => void = () => {};
</script>

<header class="h-16 bg-base-100 border-b border-base-300 flex items-center justify-between px-4 lg:px-6">
  <div class="flex items-center gap-4">
    <!-- Hamburger menu (mobile only) -->
    <button
      class="btn btn-ghost btn-square lg:hidden"
      on:click={onMenuClick}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        stroke-width="1.5"
        stroke="currentColor"
        class="w-6 h-6"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5"
        />
      </svg>
    </button>

    <div class="overflow-x-auto">
      <slot name="breadcrumbs" />
    </div>
  </div>

  <div class="flex items-center gap-2 lg:gap-4">
    {#if $currentUser}
      <div class="dropdown dropdown-end">
        <label tabindex="0" class="btn btn-ghost btn-circle avatar placeholder">
          <div class="bg-neutral-focus text-neutral-content rounded-full w-8 lg:w-10">
            <span class="text-lg lg:text-xl">{$currentUser.display_name[0].toUpperCase()}</span>
          </div>
        </label>
        <ul
          tabindex="0"
          class="mt-3 p-2 shadow menu menu-compact dropdown-content bg-base-100 rounded-box w-52"
        >
          <li class="menu-title">
            <span class="truncate">{$currentUser.email}</span>
          </li>
          <li><a href="/settings">Settings</a></li>
        </ul>
      </div>
    {/if}
  </div>
</header>
