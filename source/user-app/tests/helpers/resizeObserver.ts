// jsdom ships no ResizeObserver, and the deep-link scroll (issue #1176) is
// deliberately layout-driven — it re-scrolls when an observed box changes
// size. This fake records what it was asked to observe and lets a test fire
// the callback on demand, which is how "a sibling settled late and pushed the
// target off-screen" is expressed without a real layout engine.

/** Controllable stand-in for the platform `ResizeObserver`. */
export class FakeResizeObserver {
  /** Every observer constructed since the last `reset()`, in order. */
  static instances: FakeResizeObserver[] = [];

  readonly targets: Element[] = [];
  disconnected = false;

  constructor(private readonly callback: ResizeObserverCallback) {
    FakeResizeObserver.instances.push(this);
  }

  observe(target: Element) {
    this.targets.push(target);
  }

  unobserve() {}

  disconnect() {
    this.disconnected = true;
  }

  /** Pretend an observed box changed size; a disconnected observer is inert. */
  fire() {
    if (this.disconnected) return;
    this.callback([], this as unknown as ResizeObserver);
  }

  /** The observer currently driving the page. */
  static get last(): FakeResizeObserver {
    const observer = FakeResizeObserver.instances.at(-1);
    if (!observer) throw new Error("no ResizeObserver was created");
    return observer;
  }

  static reset() {
    FakeResizeObserver.instances = [];
  }
}
