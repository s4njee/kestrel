// SessionTabs.svelte.test.ts — Component tests for the host-tab strip (E8-S9).

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SessionTabs from "./SessionTabs.svelte";
import type { SessionEntry, ConnState } from "$lib/stores/sessions.svelte";

/**
 * Build a tracked session entry.
 *
 * @param id - the session id.
 * @param over - overrides for host/username/state.
 * @returns a SessionEntry.
 */
function entry(
  id: string,
  over: { host?: string; username?: string; state?: ConnState } = {},
): SessionEntry {
  return {
    info: {
      id,
      host: over.host ?? "example.com",
      port: 22,
      username: over.username ?? "deploy",
    },
    state: over.state ?? "connected",
  } as SessionEntry;
}

/**
 * Render the strip.
 *
 * @param entries - the sessions to show.
 * @param activeId - the current tab.
 * @returns the props, whose callbacks are spies.
 */
function renderTabs(entries: SessionEntry[], activeId: string | null = "s1") {
  const props = {
    entries,
    activeId,
    onSelect: vi.fn(),
    onClose: vi.fn(),
    onNew: vi.fn(),
  };
  render(SessionTabs, { props });
  return props;
}

describe("SessionTabs", () => {
  it("renders nothing at all when no session is connected", () => {
    renderTabs([]);
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
  });

  it("shows one tab per session, labelled user@host", () => {
    renderTabs([
      entry("s1", { host: "a.example", username: "root" }),
      entry("s2", { host: "b.example", username: "deploy" }),
    ]);
    expect(screen.getByRole("tab", { name: /root@a\.example/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /deploy@b\.example/ })).toBeInTheDocument();
  });

  it("distinguishes two sessions to the same host by user", () => {
    renderTabs([
      entry("s1", { host: "same.example", username: "root" }),
      entry("s2", { host: "same.example", username: "deploy" }),
    ]);
    expect(screen.getByRole("tab", { name: /root@same/ })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /deploy@same/ })).toBeInTheDocument();
  });

  it("marks exactly one tab selected", () => {
    renderTabs([entry("s1"), entry("s2")], "s2");
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.getAttribute("aria-selected"))).toEqual(["false", "true"]);
  });

  it("selects a session when its tab is clicked", async () => {
    const props = renderTabs([
      entry("s1", { host: "a.example" }),
      entry("s2", { host: "b.example" }),
    ]);
    await fireEvent.click(screen.getByRole("tab", { name: /deploy@b\.example/ }));
    expect(props.onSelect).toHaveBeenCalledWith("s2");
  });

  it("closes only the session whose × is clicked", async () => {
    const props = renderTabs([
      entry("s1", { host: "a.example" }),
      entry("s2", { host: "b.example" }),
    ]);
    await fireEvent.click(screen.getByLabelText("Disconnect deploy@b.example"));
    expect(props.onClose).toHaveBeenCalledWith("s2");
    expect(props.onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps a reconnecting session's tab, marked rather than removed", () => {
    renderTabs([entry("s1", { state: "reconnecting" })]);
    // The tab must not vanish under the user while the supervisor retries.
    expect(screen.getByRole("tab")).toHaveTextContent("⟳");
  });

  it("offers a way to open another session", async () => {
    const props = renderTabs([entry("s1")]);
    await fireEvent.click(screen.getByLabelText("Connect to another host"));
    expect(props.onNew).toHaveBeenCalled();
  });

  it("still shows the strip for a single session, since [+] lives there", () => {
    renderTabs([entry("s1")]);
    expect(screen.getByRole("tablist")).toBeInTheDocument();
  });
});
