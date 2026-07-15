// vite.config.js — Vite + Vitest configuration for the sftpapp frontend.
//
// Serves the SvelteKit SPA in dev (fixed port 1420 that Tauri attaches to) and
// configures Vitest for unit/component tests. The `svelteTesting()` plugin +
// jsdom environment let @testing-library/svelte mount Svelte 5 components in
// tests. `defineConfig` comes from vitest/config so the `test` block typechecks.

/// <reference types="vitest/config" />
import { defineConfig } from "vitest/config";
import { sveltekit } from "@sveltejs/kit/vite";
import { svelteTesting } from "@testing-library/svelte/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit(), svelteTesting()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },

  // Vitest: jsdom for component tests; shared setup registers jest-dom matchers.
  // A concrete origin (not the opaque about:blank default) is required for
  // jsdom to enable window.localStorage, which the ui store persists to.
  test: {
    environment: "jsdom",
    environmentOptions: { jsdom: { url: "http://localhost/" } },
    globals: true,
    setupFiles: ["./vitest-setup.ts"],
    include: ["src/**/*.{test,spec}.{js,ts}"],
  },
}));
