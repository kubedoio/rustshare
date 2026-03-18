<script lang="ts">
  import { currentUser } from '$lib/stores/auth';
</script>

<header class="h-16 bg-base-100 border-b border-base-300 flex items-center justify-between px-6">
  <div class="flex items-center gap-4">
    <slot name="breadcrumbs" />
  </div>

  <div class="flex items-center gap-4">
    {#if $currentUser}
      <div class="dropdown dropdown-end">
        <label tabindex="0" class="btn btn-ghost btn-circle avatar placeholder">
          <div class="bg-neutral-focus text-neutral-content rounded-full w-10">
            <span class="text-xl">{$currentUser.display_name[0].toUpperCase()}</span>
          </div>
        </label>
        <ul
          tabindex="0"
          class="mt-3 p-2 shadow menu menu-compact dropdown-content bg-base-100 rounded-box w-52"
        >
          <li class="menu-title">
            <span>{$currentUser.email}</span>
          </li>
          <li><a href="/settings">Settings</a></li>
        </ul>
      </div>
    {/if}
  </div>
</header>
