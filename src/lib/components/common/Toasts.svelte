<!--
  Toasts.svelte — Transient notification stack.

  Renders the toasts store as a fixed bottom-right stack; each toast is a button
  that dismisses on click. Purely presentational — content and lifetime come from
  the toasts store.
-->
<script lang="ts">
  import { toasts } from "$lib/stores/toasts.svelte";
</script>

{#if toasts.items.length > 0}
  <div class="toasts" role="region" aria-label="Notifications">
    {#each toasts.items as toast (toast.id)}
      <button
        type="button"
        class="toast {toast.kind}"
        onclick={() => toasts.dismiss(toast.id)}
        title="Dismiss"
      >
        {toast.message}
      </button>
    {/each}
  </div>
{/if}

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 1000;
    max-width: min(420px, 90vw);
  }
  .toast {
    text-align: left;
    padding: 10px 14px;
    border-radius: 8px;
    border: 1px solid transparent;
    font-size: 0.82rem;
    color: #fff;
    cursor: pointer;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.18);
    animation: slide-in 0.15s ease-out;
  }
  .toast.error {
    background: #c0392b;
    border-color: #a5311f;
  }
  .toast.info {
    background: #396cd8;
    border-color: #2f59b3;
  }
  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
