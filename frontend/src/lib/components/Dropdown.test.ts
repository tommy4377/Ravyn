// @vitest-environment jsdom

import { cleanup, fireEvent, render, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import Dropdown from "./Dropdown.svelte";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const options = [
  { value: "", label: "Best available quality" },
  { value: "137", label: "1080p · MP4" },
  { value: "136", label: "720p · MP4" },
];

describe("Dropdown", () => {
  it("renders a styled combobox trigger showing the selected label", () => {
    const { getByRole } = render(Dropdown, {
      props: { options, value: "137", label: "Quality" },
    });
    const trigger = getByRole("combobox", { name: "Quality" });
    expect(trigger.textContent).toContain("1080p · MP4");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens a styled listbox and commits a clicked option", async () => {
    const onchange = vi.fn();
    const { getByRole, getAllByRole } = render(Dropdown, {
      props: { options, value: "", label: "Quality", onchange },
    });
    const trigger = getByRole("combobox", { name: "Quality" });
    await fireEvent.click(trigger);

    const listbox = getByRole("listbox", { hidden: true });
    expect(listbox.getAttribute("popover")).toBe("auto");
    const rendered = getAllByRole("option", { hidden: true });
    expect(rendered).toHaveLength(3);
    expect(rendered[0]!.getAttribute("aria-selected")).toBe("true");

    await fireEvent.click(rendered[1]!);
    expect(onchange).toHaveBeenCalledWith("137");
    await waitFor(() => {
      expect(trigger.getAttribute("aria-expanded")).toBe("false");
      expect(trigger.textContent).toContain("1080p · MP4");
    });
  });

  it("supports keyboard selection from the listbox", async () => {
    const onchange = vi.fn();
    const { getByRole } = render(Dropdown, {
      props: { options, value: "", label: "Quality", onchange },
    });
    const trigger = getByRole("combobox", { name: "Quality" });
    await fireEvent.keyDown(trigger, { key: "ArrowDown" });

    const listbox = getByRole("listbox", { hidden: true });
    await fireEvent.keyDown(listbox, { key: "ArrowDown" });
    await fireEvent.keyDown(listbox, { key: "ArrowDown" });
    await fireEvent.keyDown(listbox, { key: "Enter" });
    expect(onchange).toHaveBeenCalledWith("136");
  });

  it("closes on Escape without changing the value", async () => {
    const onchange = vi.fn();
    const { getByRole, queryByRole } = render(Dropdown, {
      props: { options, value: "136", label: "Quality", onchange },
    });
    const trigger = getByRole("combobox", { name: "Quality" });
    await fireEvent.click(trigger);
    const listbox = getByRole("listbox", { hidden: true });
    await fireEvent.keyDown(listbox, { key: "Escape" });
    await waitFor(() => {
      expect(queryByRole("listbox", { hidden: true })).toBeNull();
    });
    expect(onchange).not.toHaveBeenCalled();
    expect(trigger.textContent).toContain("720p · MP4");
  });
});
