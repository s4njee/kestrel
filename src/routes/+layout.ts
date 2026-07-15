// +layout.ts — Root SvelteKit layout configuration.
//
// Disables server-side rendering so the app runs as a pure client-side SPA.
// Tauri has no Node.js server; adapter-static (see svelte.config.js) emits a
// single index.html fallback that the webview loads. This module exports only
// build-time flags — it has no runtime logic.

// Force client-only rendering (no SSR/prerender server pass).
export const ssr = false;
