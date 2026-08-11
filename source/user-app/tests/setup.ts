import "@testing-library/jest-dom";
// jsdom has no IndexedDB; the DNS stats layer (lib/dnsDb, lib/dnsStats) needs a
// real implementation. fake-indexeddb/auto installs a spec-compliant in-memory
// one on globalThis (indexedDB, IDBKeyRange, …) for every test.
import "fake-indexeddb/auto";

// jsdom has no ResizeObserver, and layout-driven code (the deep-link scroll in
// useHashScrollTarget) constructs one unconditionally. Install an inert
// default so a component that merely renders doesn't die with a
// ReferenceError; suites that assert on observation stub their own
// controllable one over the top (tests/helpers/resizeObserver.ts).
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
};
