import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  useDnsRecords,
  useDnsZones,
  useForwardingRules,
  useCreateDnsRecord,
  useUpdateDnsRecord,
  useDeleteDnsRecord,
  useCreateDnsZone,
  useUpdateDnsZone,
  useDeleteDnsZone,
  useCreateForwardingRule,
  useUpdateForwardingRule,
  useDeleteForwardingRule,
} = vi.hoisted(() => ({
  useDnsRecords: vi.fn(),
  useDnsZones: vi.fn(),
  useForwardingRules: vi.fn(),
  useCreateDnsRecord: vi.fn(),
  useUpdateDnsRecord: vi.fn(),
  useDeleteDnsRecord: vi.fn(),
  useCreateDnsZone: vi.fn(),
  useUpdateDnsZone: vi.fn(),
  useDeleteDnsZone: vi.fn(),
  useCreateForwardingRule: vi.fn(),
  useUpdateForwardingRule: vi.fn(),
  useDeleteForwardingRule: vi.fn(),
}));

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useDnsRecords,
    useDnsZones,
    useForwardingRules,
    useCreateDnsRecord,
    useUpdateDnsRecord,
    useDeleteDnsRecord,
    useCreateDnsZone,
    useUpdateDnsZone,
    useDeleteDnsZone,
    useCreateForwardingRule,
    useUpdateForwardingRule,
    useDeleteForwardingRule,
  };
});

vi.mock("@/components/compound/PageHeader", () => ({
  PageHeader: ({
    title,
    description,
  }: {
    title: ReactNode;
    description?: ReactNode;
  }) => (
    <div>
      <h1>{title}</h1>
      <p>{description}</p>
    </div>
  ),
}));

// The cards are mocked to thin harnesses that surface the page's wiring: they
// expose the hoisted callbacks and derived flags so the tests assert what the
// page wired, not the cards' own rendering.
vi.mock("@/components/features/LocalRecordsCard", () => ({
  LocalRecordsCard: ({
    records,
    zones,
    isSaving,
    onCreateRecord,
    onUpdateRecord,
    onDeleteRecord,
  }: {
    records: Array<{ id: string }>;
    zones: Array<{ id: string }>;
    isSaving: boolean;
    onCreateRecord: (body: unknown, cb?: unknown) => void;
    onUpdateRecord: (change: unknown, cb?: unknown) => void;
    onDeleteRecord: (id: string) => void;
  }) => (
    <div data-testid="local-records">
      <span data-testid="records-count">{records.length}</span>
      <span data-testid="records-zones">{zones.length}</span>
      <span data-testid="records-saving">{String(isSaving)}</span>
      <button onClick={() => onCreateRecord({ domain: "nas.lan" })}>
        create-record
      </button>
      <button onClick={() => onUpdateRecord({ id: "rec1", body: {} })}>
        update-record
      </button>
      <button onClick={() => onDeleteRecord("rec1")}>delete-record</button>
    </div>
  ),
}));
vi.mock("@/components/features/DnsZonesCard", () => ({
  DnsZonesCard: ({
    zones,
    records,
    onCreateZone,
    onUpdateZone,
    onDeleteZone,
  }: {
    zones: Array<{ id: string }>;
    records: Array<{ id: string }>;
    onCreateZone: (body: unknown, cb?: unknown) => void;
    onUpdateZone: (change: unknown, cb?: unknown) => void;
    onDeleteZone: (id: string) => void;
  }) => (
    <div data-testid="dns-zones">
      <span data-testid="zones-count">{zones.length}</span>
      <span data-testid="zones-records">{records.length}</span>
      <button onClick={() => onCreateZone({ name: "office" })}>
        create-zone
      </button>
      <button onClick={() => onUpdateZone({ id: "z1", body: {} })}>
        update-zone
      </button>
      <button onClick={() => onDeleteZone("z1")}>delete-zone</button>
    </div>
  ),
}));
vi.mock("@/components/features/ConditionalForwardingCard", () => ({
  ConditionalForwardingCard: ({
    rules,
    onCreateRule,
    onUpdateRule,
    onDeleteRule,
  }: {
    rules: Array<{ id: string }>;
    onCreateRule: (body: unknown, cb?: unknown) => void;
    onUpdateRule: (change: unknown, cb?: unknown) => void;
    onDeleteRule: (id: string) => void;
  }) => (
    <div data-testid="conditional-forwarding">
      <span data-testid="rules-count">{rules.length}</span>
      <button onClick={() => onCreateRule({ domain: "lab.internal" })}>
        create-rule
      </button>
      <button onClick={() => onUpdateRule({ id: "r1", body: {} })}>
        update-rule
      </button>
      <button onClick={() => onDeleteRule("r1")}>delete-rule</button>
    </div>
  ),
}));
// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
vi.mock("@/components/features/DhcpLanInfoCard", () => ({
  DhcpLanInfoCard: ({ dhcpRecordCount }: { dhcpRecordCount: number }) => (
    <div data-testid="dhcp-lan-info">{dhcpRecordCount}</div>
  ),
}));

import DnsLocal from "@/pages/DnsLocal";
import { renderWithProviders } from "../test-utils";

const createRecordMutate = vi.fn();
const updateRecordMutate = vi.fn();
const deleteRecordMutate = vi.fn();
const createZoneMutate = vi.fn();
const updateZoneMutate = vi.fn();
const deleteZoneMutate = vi.fn();
const createRuleMutate = vi.fn();
const updateRuleMutate = vi.fn();
const deleteRuleMutate = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  useDnsRecords.mockReturnValue({
    data: {
      records: [
        { id: "rec1", source: "manual" },
        { id: "rec2", source: "dhcp" },
        { id: "rec3", source: "dhcp" },
      ],
    },
  });
  useDnsZones.mockReturnValue({ data: { zones: [{ id: "z1" }] } });
  useForwardingRules.mockReturnValue({ data: { rules: [{ id: "r1" }] } });
  useCreateDnsRecord.mockReturnValue({
    mutate: createRecordMutate,
    isPending: false,
  });
  useUpdateDnsRecord.mockReturnValue({
    mutate: updateRecordMutate,
    isPending: false,
  });
  useDeleteDnsRecord.mockReturnValue({ mutate: deleteRecordMutate });
  useCreateDnsZone.mockReturnValue({
    mutate: createZoneMutate,
    isPending: false,
  });
  useUpdateDnsZone.mockReturnValue({
    mutate: updateZoneMutate,
    isPending: false,
  });
  useDeleteDnsZone.mockReturnValue({ mutate: deleteZoneMutate });
  useCreateForwardingRule.mockReturnValue({
    mutate: createRuleMutate,
    isPending: false,
  });
  useUpdateForwardingRule.mockReturnValue({
    mutate: updateRuleMutate,
    isPending: false,
  });
  useDeleteForwardingRule.mockReturnValue({ mutate: deleteRuleMutate });
});

describe("DnsLocal", () => {
  it("renders the header and composes every child card", () => {
    renderWithProviders(<DnsLocal />);
    expect(
      screen.getByRole("heading", { name: "Local DNS" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("local-records")).toBeInTheDocument();
    expect(screen.getByTestId("dns-zones")).toBeInTheDocument();
    expect(screen.getByTestId("conditional-forwarding")).toBeInTheDocument();
    expect(screen.getByTestId("dhcp-lan-info")).toBeInTheDocument();
  });

  it("shares the records and zones queries across the cards", () => {
    renderWithProviders(<DnsLocal />);
    // Full record list goes to both cards; the DHCP count is derived once.
    expect(screen.getByTestId("records-count")).toHaveTextContent("3");
    expect(screen.getByTestId("zones-records")).toHaveTextContent("3");
    expect(screen.getByTestId("records-zones")).toHaveTextContent("1");
    expect(screen.getByTestId("zones-count")).toHaveTextContent("1");
    expect(screen.getByTestId("dhcp-lan-info")).toHaveTextContent("2");
  });

  it("wires the record mutations to the LocalRecordsCard callbacks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsLocal />);
    await user.click(screen.getByText("create-record"));
    expect(createRecordMutate).toHaveBeenCalledWith({ domain: "nas.lan" });
    await user.click(screen.getByText("update-record"));
    expect(updateRecordMutate).toHaveBeenCalledWith({ id: "rec1", body: {} });
    await user.click(screen.getByText("delete-record"));
    expect(deleteRecordMutate).toHaveBeenCalledWith("rec1");
  });

  it("wires the zone mutations to the DnsZonesCard callbacks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsLocal />);
    await user.click(screen.getByText("create-zone"));
    expect(createZoneMutate).toHaveBeenCalledWith({ name: "office" });
    await user.click(screen.getByText("update-zone"));
    expect(updateZoneMutate).toHaveBeenCalledWith({ id: "z1", body: {} });
    await user.click(screen.getByText("delete-zone"));
    expect(deleteZoneMutate).toHaveBeenCalledWith("z1");
  });

  it("wires the forwarding-rule mutations to the card callbacks", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsLocal />);
    await user.click(screen.getByText("create-rule"));
    expect(createRuleMutate).toHaveBeenCalledWith({ domain: "lab.internal" });
    await user.click(screen.getByText("update-rule"));
    expect(updateRuleMutate).toHaveBeenCalledWith({ id: "r1", body: {} });
    await user.click(screen.getByText("delete-rule"));
    expect(deleteRuleMutate).toHaveBeenCalledWith("r1");
  });

  it("falls back to empty data while the queries are unresolved", () => {
    useDnsRecords.mockReturnValue({ data: undefined });
    useDnsZones.mockReturnValue({ data: undefined });
    useForwardingRules.mockReturnValue({ data: undefined });
    renderWithProviders(<DnsLocal />);
    expect(screen.getByTestId("records-count")).toHaveTextContent("0");
    expect(screen.getByTestId("zones-count")).toHaveTextContent("0");
    expect(screen.getByTestId("rules-count")).toHaveTextContent("0");
    expect(screen.getByTestId("dhcp-lan-info")).toHaveTextContent("0");
  });

  it("derives isSaving from the create/update record mutations", () => {
    useCreateDnsRecord.mockReturnValue({
      mutate: createRecordMutate,
      isPending: true,
    });
    renderWithProviders(<DnsLocal />);
    expect(screen.getByTestId("records-saving")).toHaveTextContent("true");
  });
});
