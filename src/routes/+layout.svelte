<!--
  +layout.svelte — Root layout wrapper.

  Imports the global stylesheet once and renders the active route. There is a
  single route in this SPA (see +layout.ts, ssr = false), so this just hosts the
  page.
-->
<script lang="ts">
  import "../app.css";
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
  }

  let { children }: Props = $props();
</script>

{@render children()}
