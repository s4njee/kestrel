// smoke.e2e.ts — End-to-end smoke: launch, connect, download one file.
//
// Assumes a reachable SFTP server (the CI job runs atmoz/sftp) configured via
// env: E2E_HOST/PORT/USER/PASS, with a file to download in the writable dir.
// This is a best-effort smoke check (the CI job is non-blocking) — it exercises
// the real webview against a real server through tauri-driver.

const HOST = process.env.E2E_HOST ?? "127.0.0.1";
const PORT = process.env.E2E_PORT ?? "2222";
const USER = process.env.E2E_USER ?? "user";
const PASS = process.env.E2E_PASS ?? "pass";

describe("Kestrel smoke", () => {
  it("launches and shows the toolbar", async () => {
    await expect($("header .scheme")).toHaveText("kestrel://");
    await expect($("button=Connect…")).toBeDisplayed();
  });

  it("connects and downloads a file", async () => {
    // Open the connect dialog.
    await $("button=Connect…").click();
    await $('input[placeholder="example.com"]').setValue(HOST);

    // Port + username are the remaining text inputs in the form.
    const inputs = await $$('input[type="number"], input:not([type])');
    // Port (number input) then username (untyped input).
    await $('input[type="number"]').setValue(PORT);
    await inputs[inputs.length - 1].setValue(USER);
    await $('input[type="password"]').setValue(PASS);

    await $("button=Connect").click();

    // Accept the unknown host key (TOFU) if prompted.
    const trust = $("button=Trust & connect");
    if (await trust.isExisting()) await trust.click();

    // The remote pane should list at least one file; select the first one.
    const firstFile = $("section[data-kind='remote'] button.row[data-row-kind='file']");
    await firstFile.waitForDisplayed({ timeout: 30_000 });
    await firstFile.click();

    // Download it and wait for a transfer row to reach the done state.
    await $("button=Download ↓").click();
    const doneRow = $("div.row[data-state='done']");
    await doneRow.waitForDisplayed({ timeout: 60_000 });
    await expect(doneRow).toBeDisplayed();
  });
});
