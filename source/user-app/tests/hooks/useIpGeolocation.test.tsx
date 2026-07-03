import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

import { useIpGeolocation } from "../../src/hooks/useIpGeolocation";
import { createQueryWrapper } from "../test-utils";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useIpGeolocation", () => {
  it("fetches and returns the geolocation payload", async () => {
    const payload = {
      ip: "1.2.3.4",
      country_code: "US",
      country_name: "United States",
      city: "NYC",
      org: "ISP",
      latitude: 40,
      longitude: -74,
    };
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: true,
      json: async () => payload,
    } as Response);

    const { result } = renderHook(() => useIpGeolocation(), {
      wrapper: createQueryWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(payload);
    expect(fetchSpy).toHaveBeenCalledWith("https://ipapi.co/json/");
  });

  it("errors when the response is not ok", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue({
      ok: false,
      status: 500,
    } as Response);

    const { result } = renderHook(() => useIpGeolocation(), {
      wrapper: createQueryWrapper(),
    });

    // The hook sets retry: 1, so the error settles after one backoff.
    await waitFor(() => expect(result.current.isError).toBe(true), {
      timeout: 5000,
    });
    expect(result.current.error).toBeInstanceOf(Error);
    expect((result.current.error as Error).message).toContain("500");
  });
});
