// vitest-setup.ts — Global test setup, loaded before every test file.
//
// Registers @testing-library/jest-dom's custom matchers (toBeInTheDocument,
// toHaveTextContent, …) on Vitest's expect. Also installs a minimal in-memory
// localStorage polyfill: this jsdom build does not expose window.localStorage
// (opaque-origin behavior), and Node's experimental global is inert, so the ui
// store's persistence would otherwise have nothing to write to under test.

import "@testing-library/jest-dom/vitest";

/** Map-backed Storage implementation sufficient for tests. */
class MemoryStorage implements Storage {
  #map = new Map<string, string>();

  get length(): number {
    return this.#map.size;
  }
  clear(): void {
    this.#map.clear();
  }
  getItem(key: string): string | null {
    return this.#map.has(key) ? (this.#map.get(key) as string) : null;
  }
  key(index: number): string | null {
    return Array.from(this.#map.keys())[index] ?? null;
  }
  removeItem(key: string): void {
    this.#map.delete(key);
  }
  setItem(key: string, value: string): void {
    this.#map.set(key, String(value));
  }
}

if (typeof window !== "undefined" && !window.localStorage) {
  Object.defineProperty(window, "localStorage", {
    value: new MemoryStorage(),
    configurable: true,
  });
}
