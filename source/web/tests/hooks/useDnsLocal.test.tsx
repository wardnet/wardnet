import { renderHook, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createQueryWrapper } from "../test-utils";

const { dnsLocalService, toast } = vi.hoisted(() => ({
  dnsLocalService: {
    listZones: vi.fn(),
    getZone: vi.fn(),
    listZoneRecords: vi.fn(),
    createZone: vi.fn(),
    updateZone: vi.fn(),
    deleteZone: vi.fn(),
    listRecords: vi.fn(),
    getRecord: vi.fn(),
    createRecord: vi.fn(),
    updateRecord: vi.fn(),
    deleteRecord: vi.fn(),
    listForwardingRules: vi.fn(),
    getForwardingRule: vi.fn(),
    createForwardingRule: vi.fn(),
    updateForwardingRule: vi.fn(),
    deleteForwardingRule: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../../src/lib/sdk", () => ({ dnsLocalService }));
vi.mock("@wardnet/ui", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast };
});

import * as l from "../../src/hooks/useDnsLocal";

const w = createQueryWrapper;

describe("useDnsLocal zones", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists zones, reads one, and lists a zone's records", async () => {
    dnsLocalService.listZones.mockResolvedValue({ zones: [] });
    dnsLocalService.getZone.mockResolvedValue({ zone: {} });
    dnsLocalService.listZoneRecords.mockResolvedValue({ records: [] });

    const { result: list } = renderHook(() => l.useDnsZones(), {
      wrapper: w(),
    });
    await waitFor(() => expect(list.current.isSuccess).toBe(true));

    const { result: one } = renderHook(() => l.useDnsZone("z1"), {
      wrapper: w(),
    });
    await waitFor(() => expect(one.current.isSuccess).toBe(true));
    expect(dnsLocalService.getZone).toHaveBeenCalledWith("z1");

    const { result: recs } = renderHook(() => l.useDnsZoneRecords("z1"), {
      wrapper: w(),
    });
    await waitFor(() => expect(recs.current.isSuccess).toBe(true));

    const { result: none } = renderHook(() => l.useDnsZone(undefined), {
      wrapper: w(),
    });
    expect(none.current.fetchStatus).toBe("idle");
  });

  it("creates, updates, and deletes a zone", async () => {
    dnsLocalService.createZone.mockResolvedValue({ message: "" });
    dnsLocalService.updateZone.mockResolvedValue({ message: "" });
    dnsLocalService.deleteZone.mockResolvedValue({ message: "" });

    const { result: c } = renderHook(() => l.useCreateDnsZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({ name: "home.arpa" } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Zone created");

    const { result: u } = renderHook(() => l.useUpdateDnsZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({ id: "z1", body: { name: "x" } });
    });
    expect(dnsLocalService.updateZone).toHaveBeenCalledWith("z1", {
      name: "x",
    });

    const { result: d } = renderHook(() => l.useDeleteDnsZone(), {
      wrapper: w(),
    });
    await act(async () => {
      await d.current.mutateAsync("z1");
    });
    expect(toast.success).toHaveBeenCalledWith("Zone deleted");
  });
});

describe("useDnsLocal records", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists records, reads one, and runs CRUD", async () => {
    dnsLocalService.listRecords.mockResolvedValue({ records: [] });
    dnsLocalService.getRecord.mockResolvedValue({ record: {} });
    dnsLocalService.createRecord.mockResolvedValue({ message: "" });
    dnsLocalService.updateRecord.mockResolvedValue({ message: "" });
    dnsLocalService.deleteRecord.mockResolvedValue({ message: "" });

    const { result: list } = renderHook(() => l.useDnsRecords(), {
      wrapper: w(),
    });
    await waitFor(() => expect(list.current.isSuccess).toBe(true));

    const { result: one } = renderHook(() => l.useDnsRecord("r1"), {
      wrapper: w(),
    });
    await waitFor(() => expect(one.current.isSuccess).toBe(true));

    const { result: c } = renderHook(() => l.useCreateDnsRecord(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({
        name: "a",
        type: "A",
        value: "1.1.1.1",
      } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Record created");

    const { result: u } = renderHook(() => l.useUpdateDnsRecord(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({ id: "r1", body: { value: "2.2.2.2" } });
    });
    expect(dnsLocalService.updateRecord).toHaveBeenCalledWith("r1", {
      value: "2.2.2.2",
    });

    const { result: d } = renderHook(() => l.useDeleteDnsRecord(), {
      wrapper: w(),
    });
    await act(async () => {
      await d.current.mutateAsync("r1");
    });
    expect(toast.success).toHaveBeenCalledWith("Record deleted");
  });
});

describe("useDnsLocal forwarding rules", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists rules, reads one, and runs CRUD", async () => {
    dnsLocalService.listForwardingRules.mockResolvedValue({ rules: [] });
    dnsLocalService.getForwardingRule.mockResolvedValue({ rule: {} });
    dnsLocalService.createForwardingRule.mockResolvedValue({ message: "" });
    dnsLocalService.updateForwardingRule.mockResolvedValue({ message: "" });
    dnsLocalService.deleteForwardingRule.mockRejectedValue(new Error("x"));

    const { result: list } = renderHook(() => l.useForwardingRules(), {
      wrapper: w(),
    });
    await waitFor(() => expect(list.current.isSuccess).toBe(true));

    const { result: one } = renderHook(() => l.useForwardingRule("fr1"), {
      wrapper: w(),
    });
    await waitFor(() => expect(one.current.isSuccess).toBe(true));

    const { result: c } = renderHook(() => l.useCreateForwardingRule(), {
      wrapper: w(),
    });
    await act(async () => {
      await c.current.mutateAsync({
        domain: "corp",
        upstream: "10.0.0.1",
      } as never);
    });
    expect(toast.success).toHaveBeenCalledWith("Forwarding rule created");

    const { result: u } = renderHook(() => l.useUpdateForwardingRule(), {
      wrapper: w(),
    });
    await act(async () => {
      await u.current.mutateAsync({
        id: "fr1",
        body: { upstream: "10.0.0.2" },
      });
    });
    expect(dnsLocalService.updateForwardingRule).toHaveBeenCalledWith("fr1", {
      upstream: "10.0.0.2",
    });

    const { result: d } = renderHook(() => l.useDeleteForwardingRule(), {
      wrapper: w(),
    });
    await act(async () => {
      await d.current.mutateAsync("fr1").catch(() => {});
    });
    expect(toast.error).toHaveBeenCalledWith(
      "Failed to delete forwarding rule",
    );
  });
});
