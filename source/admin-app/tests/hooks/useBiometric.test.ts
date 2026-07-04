import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { useBiometric } from "@/hooks/useBiometric";

const KEY = "wardnet_biometric_credential_id";

// jsdom runs on an opaque origin here, so window.localStorage is unavailable.
// Back it with a plain in-memory store for these tests.
function installLocalStorage() {
  const store = new Map<string, string>();
  const mock = {
    getItem: (k: string) => (store.has(k) ? store.get(k)! : null),
    setItem: (k: string, v: string) => void store.set(k, String(v)),
    removeItem: (k: string) => void store.delete(k),
    clear: () => store.clear(),
    key: (i: number) => [...store.keys()][i] ?? null,
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: mock,
  });
}

describe("useBiometric", () => {
  const b = useBiometric();

  beforeAll(() => installLocalStorage());

  beforeEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });
  afterEach(() => {
    delete (navigator as unknown as Record<string, unknown>).credentials;
    delete (window as unknown as Record<string, unknown>).PublicKeyCredential;
  });

  it("returns a stable reference across calls", () => {
    expect(useBiometric()).toBe(b);
  });

  describe("isAvailable", () => {
    it("is false without a platform-authenticator API", () => {
      expect(b.isAvailable()).toBe(false);
    });

    it("is true when PublicKeyCredential exposes the platform check", () => {
      (window as unknown as Record<string, unknown>).PublicKeyCredential = {
        isUserVerifyingPlatformAuthenticatorAvailable: () =>
          Promise.resolve(true),
      };
      expect(b.isAvailable()).toBe(true);
    });
  });

  describe("isRegistered / unregister", () => {
    it("reflects the stored credential id", () => {
      expect(b.isRegistered()).toBe(false);
      localStorage.setItem(KEY, "abc");
      expect(b.isRegistered()).toBe(true);
      b.unregister();
      expect(b.isRegistered()).toBe(false);
    });
  });

  describe("register", () => {
    it("persists a base64 credential id on success", async () => {
      const rawId = new Uint8Array([1, 2, 3, 4]).buffer;
      (navigator as unknown as Record<string, unknown>).credentials = {
        create: vi.fn().mockResolvedValue({ rawId }),
      };
      await b.register("alice");
      const stored = localStorage.getItem(KEY);
      expect(stored).toBe(btoa("\x01\x02\x03\x04"));
    });

    it("throws when credential creation returns null", async () => {
      (navigator as unknown as Record<string, unknown>).credentials = {
        create: vi.fn().mockResolvedValue(null),
      };
      await expect(b.register("bob")).rejects.toThrow(/returned null/);
      expect(localStorage.getItem(KEY)).toBeNull();
    });
  });

  describe("authenticate", () => {
    it("throws when no credential is registered", async () => {
      await expect(b.authenticate()).rejects.toThrow(/No biometric credential/);
    });

    it("resolves when the authenticator returns a credential", async () => {
      localStorage.setItem(KEY, btoa("\x01\x02"));
      const get = vi.fn().mockResolvedValue({ id: "x" });
      (navigator as unknown as Record<string, unknown>).credentials = { get };
      await expect(b.authenticate()).resolves.toBeUndefined();
      expect(get).toHaveBeenCalledOnce();
    });

    it("throws when the authenticator returns null", async () => {
      localStorage.setItem(KEY, btoa("\x01\x02"));
      (navigator as unknown as Record<string, unknown>).credentials = {
        get: vi.fn().mockResolvedValue(null),
      };
      await expect(b.authenticate()).rejects.toThrow(/No credential returned/);
    });
  });
});
