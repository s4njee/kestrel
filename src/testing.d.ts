// testing.d.ts — Ambient types for the test toolchain.
//
// Importing the jest-dom vitest entry pulls in its `declare module "vitest"`
// augmentation, so custom matchers (toBeInTheDocument, toHaveTextContent, …)
// typecheck under svelte-check/tsc, not just at runtime. Lives under src/ so
// the SvelteKit tsconfig includes it.

import "@testing-library/jest-dom/vitest";
