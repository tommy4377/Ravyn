// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import DialogHarness from "../../test/DialogHarness.svelte";
import TooltipHarness from "../../test/TooltipHarness.svelte";
import Menu from "./Menu.svelte";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("shared overlay components", () => {
  it("assigns a unique accessible title relationship to every dialog", () => {
    const { getAllByRole } = render(DialogHarness);
    const dialogs = getAllByRole("dialog");
    expect(dialogs).toHaveLength(2);

    const titleIds = dialogs.map((dialog) => dialog.getAttribute("aria-labelledby"));
    expect(new Set(titleIds).size).toBe(2);
    for (const titleId of titleIds) {
      expect(titleId).toBeTruthy();
      expect(document.getElementById(titleId!)).not.toBeNull();
    }
  });

  it("preserves existing descriptions while a tooltip is focus-visible", async () => {
    const { getByRole, queryByRole } = render(TooltipHarness);
    const button = getByRole("button", { name: "Target" });

    await fireEvent.focusIn(button);
    const tooltip = getByRole("tooltip");
    expect(button.getAttribute("aria-describedby")?.split(/\s+/)).toEqual(
      expect.arrayContaining(["existing-description", tooltip.id]),
    );

    await fireEvent.focusOut(button);
    expect(queryByRole("tooltip")).toBeNull();
    expect(button.getAttribute("aria-describedby")).toBe("existing-description");
  });

  it("clamps menus to the viewport and focuses the first enabled item", async () => {
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 800 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 600 });
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      x: 0,
      y: 0,
      top: 0,
      right: 200,
      bottom: 100,
      left: 0,
      width: 200,
      height: 100,
      toJSON: () => ({}),
    });

    const { getByRole } = render(Menu, {
      props: {
        open: true,
        x: 790,
        y: 590,
        onClose: vi.fn(),
        items: [
          { id: "disabled", label: "Disabled", disabled: true },
          { id: "enabled", label: "Enabled" },
        ],
      },
    });

    const menu = getByRole("menu");
    const enabled = getByRole("menuitem", { name: "Enabled" });
    await waitFor(() => {
      expect(menu.getAttribute("style")).toContain("left: 592px");
      expect(menu.getAttribute("style")).toContain("top: 492px");
      expect(document.activeElement).toBe(enabled);
    });
  });
});
