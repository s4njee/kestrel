// vitest-setup.ts — Global test setup, loaded before every test file.
//
// Registers @testing-library/jest-dom's custom matchers (toBeInTheDocument,
// toHaveTextContent, …) on Vitest's expect, and auto-cleans mounted components
// between tests. Referenced by vite.config.js `test.setupFiles`.

import "@testing-library/jest-dom/vitest";
