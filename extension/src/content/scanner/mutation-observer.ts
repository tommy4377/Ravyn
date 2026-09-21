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
    // A prior run that tripped the maximumMutations cap left this non-zero;
    // without resetting, the very first mutation batch after any future
    // start() (e.g. the user re-enabling "Monitor page") would immediately
    // exceed the already-spent budget and call stop() again — permanently
    // disabling monitoring for the tab despite the UI showing it re-enabled.
    this.mutations = 0;
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
