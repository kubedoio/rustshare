<script lang="ts">
  import type { File } from '$lib/api/types';
  import { formatFileSize } from '$lib/utils/format';
  import { getOfficeFileType } from '$lib/utils/format';
  import { FileText, FileSpreadsheet, Presentation } from 'lucide-svelte';
  
  export let file: File;
  
  $: officeType = getOfficeFileType(file.mime_type, file.name);
  
  const officeConfig = {
    word: {
      icon: FileText,
      label: 'Microsoft Word Document',
      color: 'text-blue-500',
      bgColor: 'bg-blue-50'
    },
    excel: {
      icon: FileSpreadsheet,
      label: 'Microsoft Excel Spreadsheet',
      color: 'text-green-500',
      bgColor: 'bg-green-50'
    },
    powerpoint: {
      icon: Presentation,
      label: 'Microsoft PowerPoint Presentation',
      color: 'text-orange-500',
      bgColor: 'bg-orange-50'
    }
  };
  
  $: config = officeType ? officeConfig[officeType] : null;
</script>

<div class="flex flex-col items-center justify-center p-12 text-center">
  {#if config}
    <div class="w-24 h-24 rounded-2xl {config.bgColor} flex items-center justify-center mb-6">
      <svelte:component this={config.icon} size={48} class={config.color} />
    </div>
    <h3 class="text-xl font-semibold text-base-content mb-2">{file.name}</h3>
    <p class="text-base-content/60 mb-1">{config.label}</p>
    <p class="text-sm text-base-content/40 mb-6">
      {formatFileSize(file.size)} • {new Date(file.modified_at).toLocaleDateString()}
    </p>
    <div class="flex flex-col items-center gap-4">
      <p class="text-sm text-base-content/60 max-w-sm">
        This file type cannot be previewed in the browser. 
        Please download the file to view it.
      </p>
      <slot name="download-button" />
    </div>
  {:else}
    <div class="w-20 h-20 rounded-2xl bg-base-300 flex items-center justify-center mb-4">
      <svg xmlns="http://www.w3.org/2000/svg" class="w-10 h-10 text-base-content/40" fill="none" viewBox="0 0 24 24" stroke="currentColor">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
      </svg>
    </div>
    <h3 class="text-lg font-semibold text-base-content mb-2">{file.name}</h3>
    <p class="text-sm text-base-content/60">Office Document</p>
  {/if}
</div>
