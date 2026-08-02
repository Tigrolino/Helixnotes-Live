<script lang="ts">
  import type { NotebookIconId } from '$lib/utils/notebook-icons';

  let { icon, size = 18 }: { icon: NotebookIconId; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if icon === 'book'}
    <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
    <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z" />
  {:else if icon === 'folder'}
    <path d="M3 6.5A2.5 2.5 0 0 1 5.5 4H9l2 2.5h7.5A2.5 2.5 0 0 1 21 9v8.5a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5Z" />
  {:else if icon === 'briefcase'}
    <rect x="3" y="7" width="18" height="13" rx="2" />
    <path d="M8 7V5a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M3 12h18M10 12v2h4v-2" />
  {:else if icon === 'home'}
    <path d="m3 11 9-8 9 8" />
    <path d="M5 10v11h14V10M9 21v-7h6v7" />
  {:else if icon === 'star'}
    <path d="m12 2 3.1 6.3 6.9 1-5 4.8 1.2 6.9-6.2-3.3L5.8 21 7 14.1l-5-4.8 6.9-1Z" />
  {:else if icon === 'heart'}
    <path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1.1-1.1a5.5 5.5 0 0 0-7.8 7.8l1.1 1.1L12 21l7.8-7.5 1.1-1.1a5.5 5.5 0 0 0-.1-7.8Z" />
  {:else if icon === 'lightbulb'}
    <path d="M9 18h6M10 22h4" />
    <path d="M8.2 15.5A7 7 0 1 1 15.8 15.5c-.7.5-.8 1.1-.8 2.5H9c0-1.4-.1-2-.8-2.5Z" />
  {:else if icon === 'code'}
    <path d="m8 9-4 3 4 3M16 9l4 3-4 3M14 5l-4 14" />
  {:else if icon === 'flask'}
    <path d="M9 3h6M10 3v6l-5.5 9.5A1.7 1.7 0 0 0 6 21h12a1.7 1.7 0 0 0 1.5-2.5L14 9V3" />
    <path d="M7 16h10" />
  {:else if icon === 'graduation'}
    <path d="m2 9 10-5 10 5-10 5Z" />
    <path d="M6 11.5V16c3.5 2.7 8.5 2.7 12 0v-4.5M22 9v6" />
  {:else if icon === 'calendar'}
    <rect x="3" y="5" width="18" height="16" rx="2" />
    <path d="M16 3v4M8 3v4M3 10h18M8 14h.01M12 14h.01M16 14h.01M8 18h.01M12 18h.01" />
  {:else if icon === 'tasks'}
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <path d="m7 8 1.5 1.5L11 7M13 9h4M7 14l1.5 1.5L11 13M13 15h4" />
  {:else if icon === 'archive'}
    <path d="M4 8v12h16V8M3 4h18v4H3ZM9 12h6" />
  {:else if icon === 'globe'}
    <circle cx="12" cy="12" r="9" />
    <path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18" />
  {:else if icon === 'plane'}
    <path d="M22 2 9.5 14.5M22 2l-7 20-4-8-8-4Z" />
  {:else}
    <path d="M12 3a9 9 0 1 0 0 18h1.5a1.5 1.5 0 0 0 0-3H12a1.5 1.5 0 0 1 0-3h3a6 6 0 0 0 0-12Z" />
    <path d="M7.5 10h.01M9.5 6.5h.01M14.5 6.5h.01M17 10h.01" />
  {/if}
</svg>
