import { describe, expect, it } from "vitest";
import { SelectionStore } from "./selection.svelte";

const ORDER = ["a", "b", "c", "d", "e"];

describe("SelectionStore", () => {
  it("selectOnly replaces the whole selection and sets the anchor", () => {
    const selection = new SelectionStore();
    selection.selectOnly("a");
    selection.toggle("b"); // simulate a stray toggle before a plain click
    selection.selectOnly("c");
    expect([...selection.ids]).toEqual(["c"]);
    expect(selection.anchorId).toBe("c");
  });

  it("toggle adds/removes a single id without disturbing the rest", () => {
    const selection = new SelectionStore();
    selection.selectOnly("a");
    selection.toggle("b");
    expect([...selection.ids].sort()).toEqual(["a", "b"]);
    selection.toggle("a");
    expect([...selection.ids]).toEqual(["b"]);
  });

  it("selectRange selects the contiguous span between anchor and target", () => {
    const selection = new SelectionStore();
    selection.selectOnly("b");
    selection.selectRange("d", ORDER);
    expect([...selection.ids].sort()).toEqual(["b", "c", "d"]);
  });

  it("selectRange works in either direction from the anchor", () => {
    const selection = new SelectionStore();
    selection.selectOnly("d");
    selection.selectRange("b", ORDER);
    expect([...selection.ids].sort()).toEqual(["b", "c", "d"]);
  });

  it("selectAll selects every id in the given order", () => {
    const selection = new SelectionStore();
    selection.selectAll(ORDER);
    expect(selection.size).toBe(ORDER.length);
  });

  it("reconcile drops ids that no longer exist after a reload", () => {
    const selection = new SelectionStore();
    selection.selectAll(ORDER);
    selection.reconcile(new Set(["a", "c"]));
    expect([...selection.ids].sort()).toEqual(["a", "c"]);
  });

  it("clear empties the selection and anchor", () => {
    const selection = new SelectionStore();
    selection.selectAll(ORDER);
    selection.clear();
    expect(selection.size).toBe(0);
    expect(selection.anchorId).toBeNull();
  });
});
