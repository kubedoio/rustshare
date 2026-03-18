<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let value = '';
  export let placeholder = 'Search files...';
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    search: string;
    clear: void;
  }>();

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    value = target.value;
    dispatch('search', value);
  }

  function handleClear() {
    value = '';
    dispatch('clear');
  }
</script>

<div class="form-control w-full max-w-md">
  <div class="input-group">
    <span class="bg-base-200">
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
          d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"
        />
      </svg>
    </span>
    <input
      type="text"
      {placeholder}
      class="input input-bordered w-full"
      {value}
      on:input={handleInput}
      {disabled}
    />
    {#if value}
      <button
        type="button"
        class="btn btn-ghost btn-square"
        on:click={handleClear}
        {disabled}
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
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      </button>
    {/if}
  </div>
</div>
