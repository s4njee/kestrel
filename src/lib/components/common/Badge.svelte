<!--
  Badge.svelte — Small count/label pill.

  A reusable indicator used for things like the active-transfer count in the
  status bar (E2-S3). Renders nothing when `count` is 0 and `hideZero` is set.

  Props:
  - count?: number    — numeric value to display (default 0).
  - label?: string    — optional text shown instead of / alongside the count.
  - hideZero?: boolean — when true, render nothing while count is 0 (default true).
-->
<script lang="ts">
  interface Props {
    count?: number;
    label?: string;
    hideZero?: boolean;
  }

  let { count = 0, label, hideZero = true }: Props = $props();

  // Whether the badge should render at all given the hideZero policy.
  let visible = $derived(!(hideZero && !label && count === 0));

  // The text to show: label wins, otherwise the numeric count.
  let text = $derived(label ?? String(count));
</script>

{#if visible}
  <span class="badge" data-testid="badge">{text}</span>
{/if}

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.25rem;
    height: 1.25rem;
    padding: 0 0.4rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 600;
    line-height: 1;
    background-color: #396cd8;
    color: #ffffff;
  }
</style>
