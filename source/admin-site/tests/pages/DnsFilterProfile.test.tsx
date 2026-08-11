import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const H = vi.hoisted(() => ({
  useDnsFilterProfile: vi.fn(),
  useUpdateDnsFilterProfile: vi.fn(),
  useDeleteDnsFilterProfile: vi.fn(),
  useBlocklists: vi.fn(),
  useCreateBlocklist: vi.fn(),
  useUpdateBlocklist: vi.fn(),
  useDeleteBlocklist: vi.fn(),
  useRefreshBlocklist: vi.fn(),
  useAllowlist: vi.fn(),
  useCreateAllowlistEntry: vi.fn(),
  useDeleteAllowlistEntry: vi.fn(),
  useFilterRules: vi.fn(),
  useCreateFilterRule: vi.fn(),
  useUpdateFilterRule: vi.fn(),
  useDeleteFilterRule: vi.fn(),
  navigate: vi.fn(),
  // Mutable so a spec can drive the `#blocklist-<id>` deep-link anchor.
  location: { hash: "" } as { hash: string },
}));

vi.mock("react-router", async (io) => {
  const actual = await io<typeof import("react-router")>();
  return {
    ...actual,
    useParams: () => ({ id: "prof-1" }),
    useLocation: () => H.location,
    useNavigate: () => H.navigate,
  };
});

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, ...H };
});

vi.mock("@/components/compound/DetailPageHeader", () => ({
  DetailPageHeader: ({ itemLabel }: { itemLabel: ReactNode }) => (
    <div>header:{itemLabel}</div>
  ),
}));
vi.mock("@/components/compound/CronSchedulePicker", () => ({
  CronSchedulePicker: ({ onChange }: { onChange: (value: string) => void }) => (
    <button onClick={() => onChange("0 4 * * *")}>cron</button>
  ),
}));
vi.mock("@/components/compound/ConfirmDialog", () => ({
  ConfirmDialog: ({
    open,
    title,
    onConfirm,
  }: {
    open: boolean;
    title: string;
    onConfirm: () => void;
  }) =>
    open ? (
      <div>
        <span>dialog:{title}</span>
        <button onClick={onConfirm}>confirm:{title}</button>
      </div>
    ) : null,
}));
vi.mock("@/components/compound/BlocklistTable", () => ({
  BlocklistTable: ({
    onAdd,
    onEdit,
    onToggle,
    onDelete,
    onRefresh,
    blocklists,
  }: {
    onAdd: () => void;
    onEdit: (blocklist: { id: string }) => void;
    onToggle: (blocklist: { id: string }) => void;
    onDelete: (id: string) => void;
    onRefresh: (id: string) => void;
    blocklists: Array<{ id: string }>;
  }) => (
    <div>
      <span>bl-count:{blocklists.length}</span>
      <button onClick={onAdd}>bl-add</button>
      {blocklists[0] && (
        <>
          <button onClick={() => onEdit(blocklists[0])}>bl-edit</button>
          <button onClick={() => onToggle(blocklists[0])}>bl-toggle</button>
          <button onClick={() => onDelete(blocklists[0].id)}>bl-delete</button>
          <button onClick={() => onRefresh(blocklists[0].id)}>
            bl-refresh
          </button>
        </>
      )}
    </div>
  ),
}));
vi.mock("@/components/compound/AllowlistTable", () => ({
  AllowlistTable: ({
    onAdd,
    onDelete,
    entries,
  }: {
    onAdd: () => void;
    onDelete: (id: string) => void;
    entries: Array<{ id: string }>;
  }) => (
    <div>
      <span>al-count:{entries.length}</span>
      <button onClick={onAdd}>al-add</button>
      {entries[0] && (
        <button onClick={() => onDelete(entries[0].id)}>al-delete</button>
      )}
    </div>
  ),
}));
vi.mock("@/components/compound/FilterRuleTable", () => ({
  FilterRuleTable: ({
    onAdd,
    onToggle,
    onDelete,
    rules,
  }: {
    onAdd: () => void;
    onToggle: (id: string, enabled: boolean) => void;
    onDelete: (id: string) => void;
    rules: Array<{ id: string }>;
  }) => (
    <div>
      <span>fr-count:{rules.length}</span>
      <button onClick={onAdd}>fr-add</button>
      {rules[0] && (
        <>
          <button onClick={() => onToggle(rules[0].id, false)}>
            fr-toggle
          </button>
          <button onClick={() => onDelete(rules[0].id)}>fr-delete</button>
        </>
      )}
    </div>
  ),
}));

import DnsFilterProfile from "@/pages/DnsFilterProfile";
import { renderWithProviders } from "../test-utils";

const mutation = (over: Record<string, unknown> = {}) => ({
  mutate: vi.fn(),
  mutateAsync: vi.fn().mockResolvedValue(undefined),
  reset: vi.fn(),
  isPending: false,
  isError: false,
  error: null,
  variables: undefined,
  ...over,
});

let updateProfile: ReturnType<typeof mutation>;
let deleteProfile: ReturnType<typeof mutation>;
let createBlocklist: ReturnType<typeof mutation>;
let updateBlocklist: ReturnType<typeof mutation>;
let deleteBlocklist: ReturnType<typeof mutation>;
let refreshBlocklist: ReturnType<typeof mutation>;
let createAllow: ReturnType<typeof mutation>;
let deleteAllow: ReturnType<typeof mutation>;
let createRule: ReturnType<typeof mutation>;
let updateRule: ReturnType<typeof mutation>;
let deleteRule: ReturnType<typeof mutation>;

beforeEach(() => {
  vi.clearAllMocks();
  // Plain object: clearAllMocks does not reset it, so a hash set by one spec
  // would otherwise leak into the next.
  H.location = { hash: "" };
  updateProfile = mutation();
  deleteProfile = mutation();
  createBlocklist = mutation();
  updateBlocklist = mutation();
  deleteBlocklist = mutation();
  refreshBlocklist = mutation();
  createAllow = mutation();
  deleteAllow = mutation();
  createRule = mutation();
  updateRule = mutation();
  deleteRule = mutation();

  H.useDnsFilterProfile.mockReturnValue({
    data: {
      profile: {
        id: "prof-1",
        name: "Kids",
        description: "strict",
        builtin: false,
      },
    },
    isLoading: false,
    isError: false,
  });
  H.useUpdateDnsFilterProfile.mockReturnValue(updateProfile);
  H.useDeleteDnsFilterProfile.mockReturnValue(deleteProfile);
  H.useBlocklists.mockReturnValue({
    data: {
      blocklists: [{ id: "b1", name: "SB", url: "http://sb", enabled: true }],
    },
  });
  H.useCreateBlocklist.mockReturnValue(createBlocklist);
  H.useUpdateBlocklist.mockReturnValue(updateBlocklist);
  H.useDeleteBlocklist.mockReturnValue(deleteBlocklist);
  H.useRefreshBlocklist.mockReturnValue(refreshBlocklist);
  H.useAllowlist.mockReturnValue({
    data: { entries: [{ id: "a1", domain: "ok.com" }] },
  });
  H.useCreateAllowlistEntry.mockReturnValue(createAllow);
  H.useDeleteAllowlistEntry.mockReturnValue(deleteAllow);
  H.useFilterRules.mockReturnValue({
    data: { rules: [{ id: "r1", rule_text: "||x^" }] },
  });
  H.useCreateFilterRule.mockReturnValue(createRule);
  H.useUpdateFilterRule.mockReturnValue(updateRule);
  H.useDeleteFilterRule.mockReturnValue(deleteRule);
});

describe("DnsFilterProfile", () => {
  it("renders loading state", () => {
    H.useDnsFilterProfile.mockReturnValue({ isLoading: true });
    renderWithProviders(<DnsFilterProfile />);
    expect(screen.getByText("Loading…")).toBeInTheDocument();
  });

  it("renders not-found on error", () => {
    H.useDnsFilterProfile.mockReturnValue({ isError: true, isLoading: false });
    renderWithProviders(<DnsFilterProfile />);
    expect(screen.getByText("Profile not found")).toBeInTheDocument();
  });

  it("renders the populated profile with all sub-cards", () => {
    renderWithProviders(<DnsFilterProfile />);
    expect(screen.getByText("header:Kids")).toBeInTheDocument();
    expect(screen.getByText("bl-count:1")).toBeInTheDocument();
    expect(screen.getByText("al-count:1")).toBeInTheDocument();
    expect(screen.getByText("fr-count:1")).toBeInTheDocument();
  });

  it("edits and saves the identity", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByTestId("profile-edit"));
    const name = screen.getByTestId("profile-edit-name");
    await user.clear(name);
    await user.type(name, "Teens");
    await user.click(screen.getByTestId("profile-save"));
    expect(updateProfile.mutateAsync).toHaveBeenCalledWith({
      id: "prof-1",
      body: { name: "Teens", description: undefined },
    });
  });

  it("clears the description when emptied", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByTestId("profile-edit"));
    await user.clear(screen.getByTestId("profile-edit-desc"));
    await user.click(screen.getByTestId("profile-save"));
    expect(updateProfile.mutateAsync).toHaveBeenCalledWith({
      id: "prof-1",
      body: { name: undefined, description: null },
    });
  });

  it("shows an update error and can cancel the edit", async () => {
    H.useUpdateDnsFilterProfile.mockReturnValue(
      mutation({ isError: true, error: new Error("nope") }),
    );
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByTestId("profile-edit"));
    expect(screen.getByText("nope")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByText("header:Kids")).toBeInTheDocument();
  });

  it("deletes the profile via the confirm dialog", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByTestId("profile-edit"));
    await user.click(screen.getByTestId("profile-delete"));
    await user.click(screen.getByText("confirm:Delete profile"));
    expect(deleteProfile.mutateAsync).toHaveBeenCalledWith("prof-1");
    expect(H.navigate).toHaveBeenCalledWith("/dns/filter");
  });

  it("renders a builtin profile without an edit action", () => {
    H.useDnsFilterProfile.mockReturnValue({
      data: {
        profile: {
          id: "prof-1",
          name: "Base",
          description: null,
          builtin: true,
        },
      },
      isLoading: false,
      isError: false,
    });
    renderWithProviders(<DnsFilterProfile />);
    expect(screen.getByText("Builtin")).toBeInTheDocument();
    expect(screen.queryByTestId("profile-edit")).not.toBeInTheDocument();
  });

  it("adds a blocklist through the inline form", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByText("bl-add"));
    await user.type(screen.getByTestId("blocklist-name"), "List");
    await user.type(screen.getByTestId("blocklist-url"), "http://x");
    await user.click(screen.getByText("cron"));
    await user.click(screen.getByTestId("blocklist-submit"));
    expect(createBlocklist.mutateAsync).toHaveBeenCalledWith({
      name: "List",
      url: "http://x",
      cron_schedule: "0 4 * * *",
      enabled: true,
    });
  });

  it("edits an existing blocklist", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByText("bl-edit"));
    await user.click(screen.getByTestId("blocklist-submit"));
    expect(updateBlocklist.mutateAsync).toHaveBeenCalledWith({
      id: "b1",
      body: {
        name: "SB",
        url: "http://sb",
        cron_schedule: "0 3 * * *",
        enabled: true,
      },
    });
  });

  it("toggles, refreshes, and deletes a blocklist", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByText("bl-toggle"));
    expect(updateBlocklist.mutate).toHaveBeenCalledWith({
      id: "b1",
      body: { enabled: false },
    });
    await user.click(screen.getByText("bl-refresh"));
    expect(refreshBlocklist.mutate).toHaveBeenCalledWith("b1");
    await user.click(screen.getByText("bl-delete"));
    await user.click(screen.getByText("confirm:Delete blocklist"));
    expect(deleteBlocklist.mutate).toHaveBeenCalledWith("b1");
  });

  it("adds and deletes an allowlist entry", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByText("al-add"));
    await user.type(screen.getByTestId("allowlist-domain"), "good.com");
    await user.click(screen.getByTestId("allowlist-submit"));
    expect(createAllow.mutateAsync).toHaveBeenCalledWith({
      domain: "good.com",
      reason: undefined,
    });
    await user.click(screen.getByText("al-delete"));
    await user.click(screen.getByText("confirm:Remove allowlist entry"));
    expect(deleteAllow.mutate).toHaveBeenCalledWith("a1");
  });

  it("adds, toggles, and deletes a custom rule", async () => {
    const user = userEvent.setup();
    renderWithProviders(<DnsFilterProfile />);
    await user.click(screen.getByText("fr-add"));
    await user.type(screen.getByTestId("rule-text"), "||ads^");
    await user.click(screen.getByTestId("rule-submit"));
    expect(createRule.mutateAsync).toHaveBeenCalledWith({
      rule_text: "||ads^",
      comment: undefined,
      enabled: true,
    });
    await user.click(screen.getByText("fr-toggle"));
    expect(updateRule.mutate).toHaveBeenCalledWith({
      id: "r1",
      body: { enabled: false },
    });
    await user.click(screen.getByText("fr-delete"));
    await user.click(screen.getByText("confirm:Delete filter rule"));
    expect(deleteRule.mutate).toHaveBeenCalledWith("r1");
  });

  // An anomaly deep link lands on this page with a `#blocklist-<id>` hash,
  // because a blocklist has no page of its own. Without the scroll-and-
  // highlight the admin arrives at a profile with a dozen lists and no idea
  // which one is failing.
  it("scrolls to and highlights the blocklist named by the hash", () => {
    // jsdom implements neither; the component calls scrollIntoView directly.
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    H.location = { hash: "#blocklist-b1" };

    renderWithProviders(<DnsFilterProfile />);

    const anchor = document.getElementById("blocklist-b1");
    expect(anchor).not.toBeNull();
    expect(scrollIntoView).toHaveBeenCalled();
    expect(anchor?.closest("tr")).toHaveClass("anomaly-target");
  });

  it("leaves the rows alone when there is no blocklist hash", () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;

    renderWithProviders(<DnsFilterProfile />);

    expect(scrollIntoView).not.toHaveBeenCalled();
    expect(document.querySelector(".anomaly-target")).toBeNull();
  });
});
