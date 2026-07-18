// settings.svelte.ts — User settings (Svelte 5 runes), mirrors settings.json.
//
// Loaded once at app start; `save` persists via the backend (which also applies
// concurrency + conflict policy to the engine live). The pane stores read
// `showHidden` to filter dot-files, and the shell reads `defaultLocalDir` at
// startup. Defaults here match the backend `Settings::default`.

import { getSettings, saveSettings as saveSettingsCmd, type Settings } from "$lib/ipc/commands";

class SettingsStore {
  #value = $state<Settings>({
    concurrency: 3,
    defaultConflict: "ask",
    defaultLocalDir: null,
    showHidden: false,
    tarAcceleration: true,
  });
  #loaded = $state(false);

  /** The current settings. */
  get value(): Settings {
    return this.#value;
  }

  /** Whether the initial load has completed. */
  get loaded(): boolean {
    return this.#loaded;
  }

  /** Whether hidden (dot) files are shown in the panes. */
  get showHidden(): boolean {
    return this.#value.showHidden;
  }

  /** The directory the local pane should open on launch, if pinned. */
  get defaultLocalDir(): string | null {
    return this.#value.defaultLocalDir;
  }

  /**
   * Load settings from the backend.
   *
   * @returns a promise that resolves once settings are populated.
   */
  async load(): Promise<void> {
    this.#value = await getSettings();
    this.#loaded = true;
  }

  /**
   * Persist new settings (the backend applies runtime fields live).
   *
   * @param next - the settings to save.
   * @returns the stored (normalized) settings.
   */
  async save(next: Settings): Promise<Settings> {
    this.#value = await saveSettingsCmd(next);
    return this.#value;
  }
}

/** Application-wide settings store singleton. */
export const settings = new SettingsStore();
