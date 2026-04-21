<script lang="ts">
  interface Props {
    value?: string;
    placeholder?: string;
    disabled?: boolean;
    onSearch?: (value: string) => void;
    onClear?: () => void;
  }

  let {
    value = '',
    placeholder = 'Search files...',
    disabled = false,
    onSearch = () => {},
    onClear = () => {}
  }: Props = $props();

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    value = target.value;
    onSearch(value);
  }

  function handleClear() {
    value = '';
    onClear();
  }
</script>

<div class="form-control max-w-md w-full">
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
      oninput={handleInput}
      {disabled}
    />
    {#if value}
      <button
        type="button"
        class="btn btn-ghost btn-square"
        aria-label="Clear search"
        onclick={handleClear}
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
          <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    {/if}
  </div>
</div>
