import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import type { ReactElement } from "react";

import { useHashScrollTarget } from "../../src/hooks/useHashScrollTarget";
import { FakeResizeObserver } from "../helpers/resizeObserver";

const TARGET_HASH = "#target";
const SETTLE_WINDOW_MS = 2_000;

/**
 * Mirrors the shape the hook exists for: a target that is the last child of a
 * container it shares with siblings that settle later.
 */
function Target({ rearmOn }: { rearmOn?: unknown }) {
  const ref = useHashScrollTarget<HTMLDivElement>(TARGET_HASH, rearmOn);
  return (
    <div data-testid="container">
      <div data-testid="sibling" />
      <div data-testid="target" ref={ref} />
    </div>
  );
}

function renderAt(ui: ReactElement, route: string) {
  return render(ui, {
    wrapper: ({ children }) => (
      <MemoryRouter initialEntries={[route]}>{children}</MemoryRouter>
    ),
  });
}

let scrollIntoView: Mock<(arg?: boolean | ScrollIntoViewOptions) => void>;
let originalScrollIntoView: typeof Element.prototype.scrollIntoView;

beforeEach(() => {
  FakeResizeObserver.reset();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  originalScrollIntoView = Element.prototype.scrollIntoView;
  // jsdom doesn't implement scrollIntoView, so assign rather than spyOn.
  scrollIntoView = vi.fn();
  Element.prototype.scrollIntoView = scrollIntoView;
});

afterEach(() => {
  // A raw prototype assignment isn't undone by restoreAllMocks, and a pushed
  // fragment would otherwise leak into every later test — the hook reads
  // `window.location.hash` as well as the router's.
  Element.prototype.scrollIntoView = originalScrollIntoView;
  const url = new URL(window.location.href);
  url.hash = "";
  window.history.pushState(null, "", url);
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("useHashScrollTarget", () => {
  it("scrolls the target into view when the fragment targets it", () => {
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    expect(scrollIntoView).toHaveBeenCalledWith({
      behavior: "smooth",
      block: "start",
    });
  });

  it("does nothing when the fragment does not target it", () => {
    renderAt(<Target />, "/settings");
    expect(scrollIntoView).not.toHaveBeenCalled();
    expect(FakeResizeObserver.instances).toHaveLength(0);
  });

  it("observes the container as well as the target", () => {
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    // The siblings are what actually displace the target, and a sibling that
    // mounts late (a list rendering `null` until it has rows) can't be
    // observed directly — but it grows the container it joins.
    expect(FakeResizeObserver.last.targets).toEqual([
      screen.getByTestId("target"),
      screen.getByTestId("container"),
    ]);
  });

  it("re-scrolls when a late-settling sibling changes the layout", () => {
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    expect(scrollIntoView).toHaveBeenCalledTimes(1);

    // What the bug looked like: the cards above expand as their queries
    // resolve and push the target back below the fold.
    FakeResizeObserver.last.fire();
    FakeResizeObserver.last.fire();

    expect(scrollIntoView).toHaveBeenCalledTimes(3);
  });

  it("stops correcting once the settle window has elapsed", () => {
    vi.useFakeTimers();
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    const observer = FakeResizeObserver.last;

    vi.advanceTimersByTime(SETTLE_WINDOW_MS);
    observer.fire();

    // Layout churn long after arrival is the page's business, not ours.
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(observer.disconnected).toBe(true);
  });

  it("stops correcting as soon as the user takes over the scroll", () => {
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    const observer = FakeResizeObserver.last;

    window.dispatchEvent(new Event("wheel"));
    observer.fire();

    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(observer.disconnected).toBe(true);
  });

  it("arms on a fragment-only navigation that only fires hashchange", () => {
    // The SW's `existing.navigate(…#target)` on an already-open app is a
    // same-document navigation: `hashchange` fires, `popstate` does not, so
    // BrowserRouter's hash never updates.
    renderAt(<Target />, "/settings");
    expect(scrollIntoView).not.toHaveBeenCalled();

    const url = new URL(window.location.href);
    url.hash = TARGET_HASH;
    window.history.pushState(null, "", url);
    window.dispatchEvent(new HashChangeEvent("hashchange"));

    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    FakeResizeObserver.last.fire();
    expect(scrollIntoView).toHaveBeenCalledTimes(2);
  });

  it("re-arms when the target's own content settles after the window", () => {
    vi.useFakeTimers();
    const { rerender } = renderAt(
      <Target rearmOn={true} />,
      `/settings${TARGET_HASH}`,
    );
    vi.advanceTimersByTime(SETTLE_WINDOW_MS);
    expect(scrollIntoView).toHaveBeenCalledTimes(1);

    // A query that resolves this late replaces the placeholder the user was
    // sent to, so the scroll is worth re-issuing.
    rerender(<Target rearmOn={false} />);

    expect(scrollIntoView).toHaveBeenCalledTimes(2);
    expect(FakeResizeObserver.instances).toHaveLength(2);
    expect(FakeResizeObserver.instances[0].disconnected).toBe(true);
  });

  it("restarts the window rather than stacking observers on a repeat link", () => {
    renderAt(<Target />, `/settings${TARGET_HASH}`);
    const first = FakeResizeObserver.last;

    const url = new URL(window.location.href);
    url.hash = TARGET_HASH;
    window.history.pushState(null, "", url);
    window.dispatchEvent(new HashChangeEvent("hashchange"));

    expect(FakeResizeObserver.instances).toHaveLength(2);
    expect(first.disconnected).toBe(true);
    // Initial scroll + the re-arm; the retired observer moves nothing further.
    first.fire();
    expect(scrollIntoView).toHaveBeenCalledTimes(2);
  });

  it("stops observing when the target unmounts", () => {
    const { unmount } = renderAt(<Target />, `/settings${TARGET_HASH}`);
    const observer = FakeResizeObserver.last;
    unmount();
    expect(observer.disconnected).toBe(true);
  });
});
