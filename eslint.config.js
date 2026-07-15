// eslint.config.js — Flat ESLint config for the Svelte + TypeScript frontend.
//
// Layers: JS recommended → typescript-eslint recommended → eslint-plugin-svelte
// recommended, then a Svelte-specific block wiring the TS parser into <script
// lang="ts"> blocks, and finally the svelte/prettier compat layer that turns
// off formatting rules Prettier owns. Generated output and the Rust tree are
// ignored. This file has no exported functions — it is pure configuration.

import js from "@eslint/js";
import tseslint from "typescript-eslint";
import svelte from "eslint-plugin-svelte";
import globals from "globals";
import svelteConfig from "./svelte.config.js";

export default tseslint.config(
  {
    ignores: [
      "build/",
      ".svelte-kit/",
      "package/",
      "src-tauri/",
      "target/",
      "static/",
      "node_modules/",
      "*.config.js",
      "*.config.ts",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
  },
  {
    files: ["**/*.svelte", "**/*.svelte.ts", "**/*.svelte.js"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: [".svelte"],
        svelteConfig,
      },
    },
  },
  ...svelte.configs.prettier,
);
