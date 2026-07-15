// wdio.conf.ts — WebdriverIO config for the Tauri end-to-end smoke test.
//
// Drives the built desktop app through `tauri-driver` (a WebDriver proxy for the
// platform webview). Linux/Windows only — tauri-driver has no macOS support, so
// this runs in the Linux CI job (E5-S5) or locally on Linux. `tauri-driver` must
// be on PATH (`cargo install tauri-driver`) and the release binary must be built
// (`pnpm tauri build`) before running `pnpm test:e2e`.

import { spawn, type ChildProcess } from "node:child_process";
import path from "node:path";

/** The built release binary the driver launches. */
const application = path.resolve(import.meta.dirname, "../src-tauri/target/release/sftpapp");

/** Handle to the tauri-driver process, killed on completion. */
let tauriDriver: ChildProcess | undefined;

export const config: WebdriverIO.Config = {
  runner: "local",
  hostname: "127.0.0.1",
  port: 4444,
  specs: ["./specs/**/*.e2e.ts"],
  maxInstances: 1,
  capabilities: [
    {
      // tauri-driver reads this to launch the app under WebDriver.
      "tauri:options": { application },
    } as WebdriverIO.Capabilities,
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 120_000 },

  /** Launch tauri-driver before the session starts. */
  onPrepare: () => {
    tauriDriver = spawn("tauri-driver", [], {
      stdio: [null, process.stdout, process.stderr],
    });
  },

  /** Tear down tauri-driver after the run. */
  onComplete: () => {
    tauriDriver?.kill();
  },
};
