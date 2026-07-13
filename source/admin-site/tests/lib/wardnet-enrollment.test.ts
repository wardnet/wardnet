import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => {
  const mut = () => ({
    mutateAsync: vi.fn().mockResolvedValue({}),
    isPending: false,
  });
  return {
    requestCode: mut(),
    enroll: mut(),
    register: mut(),
    configureCf: mut(),
    checkSlug: { mutateAsync: vi.fn().mockResolvedValue({ available: true }) },
  };
});

vi.mock("@wardnet/web", () => ({
  useRequestEnrollmentCode: () => hoisted.requestCode,
  useEnrollDdns: () => hoisted.enroll,
  useRegisterDdns: () => hoisted.register,
  useConfigureCloudflare: () => hoisted.configureCf,
  useCheckDdnsSlug: () => hoisted.checkSlug,
}));

import {
  EnrollmentValidationError,
  useWardnetEnrollment,
} from "@/lib/wardnet-enrollment";

function setup() {
  const onProvisioned = vi.fn();
  const onError = vi.fn();
  const clearError = vi.fn();
  const view = renderHook(() =>
    useWardnetEnrollment({ onProvisioned, onError, clearError }),
  );
  return { view, onProvisioned, onError, clearError };
}

beforeEach(() => {
  vi.clearAllMocks();
  hoisted.requestCode.mutateAsync.mockResolvedValue({});
  hoisted.enroll.mutateAsync.mockResolvedValue({});
  hoisted.register.mutateAsync.mockResolvedValue({});
  hoisted.configureCf.mutateAsync.mockResolvedValue({});
  hoisted.checkSlug.mutateAsync.mockResolvedValue({ available: true });
});

describe("useWardnetEnrollment", () => {
  it("walks the wardnet email -> code -> slug happy path", async () => {
    const { view, onProvisioned, clearError } = setup();

    await act(async () => {
      view.result.current.setEmail("me@example.com");
    });
    await act(async () => {
      await view.result.current.sendCode();
    });
    expect(clearError).toHaveBeenCalled();
    expect(hoisted.requestCode.mutateAsync).toHaveBeenCalledWith({
      email: "me@example.com",
    });
    expect(view.result.current.wardnetStep).toBe("code");

    await act(async () => view.result.current.setCode("123456"));
    await act(async () => {
      await view.result.current.verifyCode();
    });
    expect(view.result.current.wardnetStep).toBe("slug");

    await act(async () => view.result.current.setSlug("happy-newton"));
    await act(async () => {
      await view.result.current.registerWardnet();
    });
    expect(hoisted.register.mutateAsync).toHaveBeenCalledWith({
      slug: "happy-newton",
    });
    expect(onProvisioned).toHaveBeenCalled();
  });

  // Regression: the Send code button used to be disabled while the email was
  // empty/malformed, so clicking it did nothing at all — no request, no error,
  // no feedback. `sendCode` must now reject the input itself and say why.
  it.each([
    ["empty", ""],
    ["missing an @", "not-an-email"],
    ["whitespace only", "   "],
  ])(
    "reports an invalid email (%s) instead of silently doing nothing",
    async (_label, value) => {
      const { view, onError } = setup();

      await act(async () => {
        view.result.current.setEmail(value);
      });
      await act(async () => {
        await view.result.current.sendCode();
      });

      // No request left the browser...
      expect(hoisted.requestCode.mutateAsync).not.toHaveBeenCalled();
      // ...and the user was told why, rather than nothing happening.
      expect(onError).toHaveBeenCalledWith(
        expect.any(EnrollmentValidationError),
      );
      expect(onError.mock.calls[0][0]).toHaveProperty(
        "message",
        "Enter the email address for your Wardnet account.",
      );
      // Still on the email step — we never advanced.
      expect(view.result.current.wardnetStep).toBe("email");
    },
  );

  it("trims the email before sending it", async () => {
    const { view } = setup();

    await act(async () => {
      view.result.current.setEmail("  me@example.com  ");
    });
    await act(async () => {
      await view.result.current.sendCode();
    });

    expect(hoisted.requestCode.mutateAsync).toHaveBeenCalledWith({
      email: "me@example.com",
    });
  });

  it("reports errors from each action via onError", async () => {
    hoisted.requestCode.mutateAsync.mockRejectedValueOnce(new Error("boom"));
    hoisted.enroll.mutateAsync.mockRejectedValueOnce(new Error("boom"));
    hoisted.register.mutateAsync.mockRejectedValueOnce(new Error("boom"));
    const { view, onError, onProvisioned } = setup();

    await act(async () => {
      await view.result.current.sendCode();
    });
    await act(async () => {
      await view.result.current.verifyCode();
    });
    await act(async () => {
      await view.result.current.registerWardnet();
    });
    expect(onError).toHaveBeenCalledTimes(3);
    expect(onProvisioned).not.toHaveBeenCalled();
  });

  it("configures cloudflare and reports its errors", async () => {
    const { view, onProvisioned } = setup();
    await act(async () => {
      view.result.current.setProvider("cloudflare");
      view.result.current.setToken("tok");
      view.result.current.setDomain("example.com");
    });
    await act(async () => {
      await view.result.current.enableCloudflare();
    });
    expect(hoisted.configureCf.mutateAsync).toHaveBeenCalledWith({
      token: "tok",
      domain: "example.com",
    });
    expect(onProvisioned).toHaveBeenCalled();
  });

  it("debounces the slug availability check to 'available'", async () => {
    vi.useFakeTimers();
    try {
      const onProvisioned = vi.fn();
      const onError = vi.fn();
      const clearError = vi.fn();
      const view = renderHook(() =>
        useWardnetEnrollment({ onProvisioned, onError, clearError }),
      );
      act(() => {
        view.result.current.setWardnetStep("slug");
        view.result.current.setSlug("happy-newton");
      });
      await act(async () => {
        vi.advanceTimersByTime(450);
      });
      // flush the resolved promise microtask
      await act(async () => {
        await Promise.resolve();
      });
      expect(hoisted.checkSlug.mutateAsync).toHaveBeenCalledWith(
        "happy-newton",
      );
      expect(view.result.current.availability).toBe("available");
    } finally {
      vi.useRealTimers();
    }
  });

  it("marks a client-invalid slug as 'invalid' without hitting the server", async () => {
    const { view } = setup();
    await act(async () => {
      view.result.current.setWardnetStep("slug");
      view.result.current.setSlug("no"); // too short
    });
    await waitFor(() =>
      expect(view.result.current.availability).toBe("invalid"),
    );
    expect(view.result.current.slugDisabled).toBe(true);
  });

  it("resets fields to a fresh enrollment", async () => {
    const { view } = setup();
    await act(async () => {
      view.result.current.setEmail("x@y.com");
      view.result.current.setCode("999");
      view.result.current.setToken("t");
    });
    await act(async () => view.result.current.reset());
    expect(view.result.current.email).toBe("");
    expect(view.result.current.code).toBe("");
    expect(view.result.current.token).toBe("");
    expect(view.result.current.wardnetStep).toBe("email");
  });

  it("surfaces a server error when the slug check rejects", async () => {
    hoisted.checkSlug.mutateAsync.mockRejectedValue(new Error("offline"));
    vi.useFakeTimers();
    try {
      const view = renderHook(() =>
        useWardnetEnrollment({
          onProvisioned: vi.fn(),
          onError: vi.fn(),
          clearError: vi.fn(),
        }),
      );
      act(() => {
        view.result.current.setWardnetStep("slug");
        view.result.current.setSlug("brave-curie");
      });
      await act(async () => {
        vi.advanceTimersByTime(450);
      });
      await act(async () => {
        await Promise.resolve();
      });
      expect(view.result.current.availability).toBe("error");
    } finally {
      vi.useRealTimers();
    }
  });
});
