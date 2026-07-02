import "@testing-library/jest-dom";
// jsdom has no IndexedDB; the DNS stats layer (lib/dnsDb, lib/dnsStats) needs a
// real implementation. fake-indexeddb/auto installs a spec-compliant in-memory
// one on globalThis (indexedDB, IDBKeyRange, …) for every test.
import "fake-indexeddb/auto";
