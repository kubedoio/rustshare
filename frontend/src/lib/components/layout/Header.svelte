<script lang="ts">
  import { currentUser } from '$lib/stores/auth';
  import { createEventDispatcher } from 'svelte';
  import ThemeToggle from '$lib/components/common/ThemeToggle.svelte';

  export let onMenuClick: () => void = () => {};
  export let onHelpClick: () => void = () => {};
  export let onSearchChange: ((query: string) => void) | null = null;
  export let searchQuery = '';

  const dispatch = createEventDispatcher();

  function handleSearchInput(event: Event) {
    const target = event.target as HTMLInputElement;
    searchQuery = target.value;
    if (onSearchChange) {
      onSearchChange(searchQuery);
    }
    dispatch('search', { query: searchQuery });
  }

  function clearSearch() {
    searchQuery = '';
    if (onSearchChange) {
      onSearchChange('');
    }
    dispatch('search', { query: '' });
  }
</script>

<header class="h-16 bg-base-100 border-b border-base-300 flex items-center justify-between px-4 lg:px-6 gap-4">
  <div class="flex items-center gap-4 min-w-0 flex-1">
    <!-- Hamburger menu (mobile only) -->
    <button
      class="btn btn-ghost btn-square lg:hidden flex-shrink-0"
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

    <div class="overflow-x-auto flex-shrink min-w-0">
      <slot name="breadcrumbs" />
    </div>

    <!-- Search bar (desktop) -->
    {#if onSearchChange !== null}
      <div class="hidden lg:flex flex-1 max-w-md">
        <div class="form-control w-full">
          <div class="input-group">
            <span class="bg-base-200">
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
              </svg>
            </span>
            <input
              type="text"
              placeholder="Search files and folders..."
              class="input input-bordered w-full input-sm"
              bind:value={searchQuery}
              on:input={handleSearchInput}
            />
            {#if searchQuery}
              <button class="btn btn-square btn-sm" on:click={clearSearch}>
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            {/if}
          </div>
        </div>
      </div>
    {/if}
  </div>

  <div class="flex items-center gap-2 lg:gap-4 flex-shrink-0">
    <!-- Theme toggle -->
    <ThemeToggle />

    <!-- Search button (mobile) -->
    {#if onSearchChange !== null}
      <div class="dropdown dropdown-end lg:hidden">
        <label tabindex="0" class="btn btn-ghost btn-circle btn-sm">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
            <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z" />
          </svg>
        </label>
        <div tabindex="0" class="dropdown-content z-[1] card card-compact w-64 p-2 shadow bg-base-100 mt-3">
          <div class="form-control">
            <div class="input-group">
              <input
                type="text"
                placeholder="Search..."
                class="input input-bordered w-full input-sm"
                bind:value={searchQuery}
                on:input={handleSearchInput}
              />
              {#if searchQuery}
                <button class="btn btn-square btn-sm" on:click={clearSearch}>
                  <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              {/if}
            </div>
          </div>
        </div>
      </div>
    {/if}

    <!-- Help button -->
    <button
      class="btn btn-ghost btn-circle btn-sm"
      on:click={onHelpClick}
      title="Keyboard shortcuts"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        fill="none"
        viewBox="0 0 24 24"
        stroke-width="1.5"
        stroke="currentColor"
        class="w-5 h-5"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M9.879 7.519c1.171-1.025 3.071-1.025 4.242 0 1.172 1.025 1.172 2.687 0 3.712-.203.179-.43.326-.67.442-.745.361-1.45.999-1.45 1.827v.75M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9 5.25h.008v.008H12v-.008z"
        />
      </svg>
    </button>

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
