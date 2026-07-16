export class BoundedMutationScanner {
  private observer: MutationObserver | null = null;
  private timer: number | null = null;
  private mutations = 0;

  constructor(
    private readonly callback: () => void,
    private readonly maximumMutations = 5_000,
    private readonly debounceMs = 400,
  ) {}

  start(root: Node = document.documentElement): void {
    if (this.observer) return;
    this.observer = new MutationObserver((records) => {
      this.mutations += records.length;
      if (this.mutations > this.maximumMutations) {
        this.stop();
        return;
      }
      if (this.timer !== null) window.clearTimeout(this.timer);
      this.timer = window.setTimeout(() => {
        this.timer = null;
        this.callback();
      }, this.debounceMs);
    });
    this.observer.observe(root, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["src", "srcset", "href", "data", "style"],
    });
  }

  stop(): void {
    this.observer?.disconnect();
    this.observer = null;
    if (this.timer !== null) window.clearTimeout(this.timer);
    this.timer = null;
  }
}
