/**
 * Generic multi-selection with keyboard range support (Shift+click/arrow,
 * Ctrl/Cmd+click) for virtualized list views. Callers supply the current
 * visible order so range selection stays correct across filtering/sorting.
 */

import { SvelteSet } from "svelte/reactivity";

export class SelectionStore {
  readonly ids = new SvelteSet<string>();
  anchorId = $state<string | null>(null);
  focusedId = $state<string | null>(null);

  get size(): number {
    return this.ids.size;
  }

  isSelected(id: string): boolean {
    return this.ids.has(id);
  }

  clear(): void {
    this.ids.clear();
    this.anchorId = null;
  }

  /** Plain click: select only this item. */
  selectOnly(id: string): void {
    this.ids.clear();
    this.ids.add(id);
    this.anchorId = id;
    this.focusedId = id;
  }

  /** Ctrl/Cmd+click: toggle membership without disturbing the rest. */
  toggle(id: string): void {
    if (this.ids.has(id)) {
      this.ids.delete(id);
    } else {
      this.ids.add(id);
    }
    this.anchorId = id;
    this.focusedId = id;
  }

  /** Shift+click or Shift+arrow: select the contiguous range from the anchor. */
  selectRange(id: string, order: string[]): void {
    const anchor = this.anchorId ?? id;
    const anchorIndex = order.indexOf(anchor);
    const targetIndex = order.indexOf(id);
    if (anchorIndex === -1 || targetIndex === -1) {
      this.selectOnly(id);
      return;
    }
    const [start, end] = anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
    this.ids.clear();
    for (let i = start; i <= end; i += 1) {
      this.ids.add(order[i]!);
    }
    this.focusedId = id;
  }

  selectAll(order: string[]): void {
    this.ids.clear();
    for (const id of order) this.ids.add(id);
    this.focusedId = order.at(-1) ?? null;
  }

  /** Drop ids that no longer exist in the current collection (after reload/removal). */
  reconcile(existingIds: Set<string>): void {
    for (const id of [...this.ids]) {
      if (!existingIds.has(id)) this.ids.delete(id);
    }
    if (this.anchorId && !existingIds.has(this.anchorId)) this.anchorId = null;
    if (this.focusedId && !existingIds.has(this.focusedId)) this.focusedId = null;
  }
}
