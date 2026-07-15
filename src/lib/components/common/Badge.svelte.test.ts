// Badge.svelte.test.ts — Component test proving the Svelte + jsdom harness.

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import Badge from "./Badge.svelte";

describe("Badge", () => {
  it("renders a numeric count", () => {
    render(Badge, { props: { count: 3 } });
    expect(screen.getByTestId("badge")).toHaveTextContent("3");
  });

  it("hides a zero count by default", () => {
    render(Badge, { props: { count: 0 } });
    expect(screen.queryByTestId("badge")).toBeNull();
  });

  it("prefers an explicit label over the count", () => {
    render(Badge, { props: { count: 0, label: "!" } });
    expect(screen.getByTestId("badge")).toHaveTextContent("!");
  });
});
