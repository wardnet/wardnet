import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "@wardnet/web";
import { ProviderTunnelTab } from "@/components/features/ProviderTunnelTab";
import { renderWithProviders } from "../../test-utils";

Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    unobserve() {}
    disconnect() {}
  },
);

vi.mock("@wardnet/web", async (importOriginal) => {
  const actual = await importOriginal<Record<string, unknown>>();
  return {
    ...actual,
    useProviders: vi.fn(),
    useProviderCountries: vi.fn(),
    useValidateCredentials: vi.fn(),
    useProviderServers: vi.fn(),
    useProviderSetup: vi.fn(),
  };
});

const validateAsync = vi.fn();
const serversMutate = vi.fn();
const setupAsync = vi.fn();

function setValidate(overrides: Record<string, unknown> = {}) {
  vi.mocked(useValidateCredentials).mockReturnValue({
    mutateAsync: validateAsync,
    mutate: vi.fn(),
    data: undefined,
    isPending: false,
    isError: false,
    error: null,
    ...overrides,
  } as unknown as ReturnType<typeof useValidateCredentials>);
}

function setServers(overrides: Record<string, unknown> = {}) {
  vi.mocked(useProviderServers).mockReturnValue({
    mutate: serversMutate,
    data: undefined,
    isPending: false,
    ...overrides,
  } as unknown as ReturnType<typeof useProviderServers>);
}

function setSetup(overrides: Record<string, unknown> = {}) {
  vi.mocked(useProviderSetup).mockReturnValue({
    mutateAsync: setupAsync,
    isPending: false,
    isError: false,
    error: null,
    ...overrides,
  } as unknown as ReturnType<typeof useProviderSetup>);
}

const PROVIDERS = [
  {
    id: "nord",
    name: "NordVPN",
    auth_methods: ["token"],
    icon_url: "https://x/nord.png",
    credentials_hint: "Paste your token",
  },
  { id: "mull", name: "Mullvad", auth_methods: ["credentials"] },
  { id: "both", name: "BothVPN", auth_methods: ["token", "credentials"] },
];

beforeEach(() => {
  vi.clearAllMocks();
  validateAsync.mockResolvedValue({ valid: true });
  setupAsync.mockResolvedValue({});
  vi.mocked(useProviders).mockReturnValue({
    data: { providers: PROVIDERS },
  } as unknown as ReturnType<typeof useProviders>);
  vi.mocked(useProviderCountries).mockReturnValue({
    data: undefined,
  } as unknown as ReturnType<typeof useProviderCountries>);
  setValidate();
  setServers();
  setSetup();
});

async function selectProvider(
  user: ReturnType<typeof userEvent.setup>,
  name: string,
) {
  await user.click(screen.getByRole("combobox"));
  await user.click(
    // eslint-disable-next-line security/detect-non-literal-regexp -- pattern is a hardcoded provider name literal from the test's own call sites
    await screen.findByRole("option", { name: new RegExp(name) }),
  );
}

describe("ProviderTunnelTab", () => {
  it("shows a note when no providers are available", () => {
    vi.mocked(useProviders).mockReturnValue({
      data: { providers: [] },
    } as unknown as ReturnType<typeof useProviders>);
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    expect(screen.getByText("No providers available.")).toBeInTheDocument();
  });

  it("reveals the token field for a token-only provider", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "NordVPN");
    expect(screen.getByLabelText("Access token")).toBeInTheDocument();
  });

  it("reveals username/password for a credentials-only provider", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "Mullvad");
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
    expect(screen.getByLabelText("Password")).toBeInTheDocument();
  });

  it("offers an auth-method switch when both methods are supported", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "BothVPN");
    // Default auth method is the first (token).
    expect(screen.getByLabelText("Access token")).toBeInTheDocument();
    // Two comboboxes now: provider + auth-method.
    const combos = screen.getAllByRole("combobox");
    await user.click(combos[1]);
    await user.click(
      await screen.findByRole("option", { name: "Username & password" }),
    );
    expect(screen.getByLabelText("Username")).toBeInTheDocument();
  });

  it("validates credentials and advances to server selection", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "Mullvad");
    await user.type(screen.getByLabelText("Username"), "alice");
    await user.type(screen.getByLabelText("Password"), "secret");
    await user.click(
      screen.getByRole("button", { name: "Validate credentials" }),
    );
    expect(validateAsync).toHaveBeenCalled();
    // credsValid → step 3 fields appear.
    expect(await screen.findByLabelText(/Hostname/)).toBeInTheDocument();
    expect(serversMutate).toHaveBeenCalled();
  });

  it("surfaces an invalid-credentials message", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    setValidate({ data: { valid: false, message: "Wrong username" } });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "Mullvad");
    expect(screen.getByText("Wrong username")).toBeInTheDocument();
  });

  it("shows an API error alert when validation errors", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    setValidate({ isError: true, error: new Error("network") });
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "Mullvad");
    expect(screen.getByText(/network|Validation failed/)).toBeInTheDocument();
  });

  it("creates the tunnel and calls onSuccess after setup", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onSuccess = vi.fn();
    renderWithProviders(<ProviderTunnelTab onSuccess={onSuccess} />);
    await selectProvider(user, "NordVPN");
    await user.type(screen.getByLabelText("Access token"), "tok-xyz");
    await user.click(
      screen.getByRole("button", { name: "Validate credentials" }),
    );
    await screen.findByLabelText(/Hostname/);
    // Refresh servers button lives in step 3.
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(serversMutate).toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Create tunnel" }));
    expect(setupAsync).toHaveBeenCalled();
    expect(onSuccess).toHaveBeenCalled();
  });

  it("renders a country combobox when the provider exposes countries", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    vi.mocked(useProviderCountries).mockReturnValue({
      data: { countries: [{ code: "us", name: "United States" }] },
    } as unknown as ReturnType<typeof useProviderCountries>);
    renderWithProviders(<ProviderTunnelTab onSuccess={vi.fn()} />);
    await selectProvider(user, "NordVPN");
    await user.type(screen.getByLabelText("Access token"), "tok-xyz");
    await user.click(
      screen.getByRole("button", { name: "Validate credentials" }),
    );
    // Country combobox trigger (placeholder All countries) is present.
    expect(await screen.findByText("All countries")).toBeInTheDocument();
  });

  it("selects a server and creates the tunnel with the chosen options", async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    const onSuccess = vi.fn();
    // No countries → free-text country input fallback path.
    setServers({
      data: {
        servers: [
          { id: "s1", name: "us-1", country_code: "us", load: 20 },
          { id: "s2", name: "us-2", country_code: "us", load: 85 },
        ],
      },
    });
    renderWithProviders(<ProviderTunnelTab onSuccess={onSuccess} />);
    await selectProvider(user, "NordVPN");
    await user.type(screen.getByLabelText("Access token"), "tok-xyz");
    await user.click(
      screen.getByRole("button", { name: "Validate credentials" }),
    );
    // Country free-text input (no country data) — typing reveals the server
    // select (country set + servers present + no hostname override).
    const countryInput = await screen.findByPlaceholderText(
      "Country code (optional)",
    );
    await user.type(countryInput, "us");
    // Server select now present; open it and pick the first server. This
    // renders ServerOption + LoadIndicator for both low- and high-load rows.
    const combos = screen.getAllByRole("combobox");
    await user.click(combos[combos.length - 1]);
    await user.click(await screen.findByRole("option", { name: /us-1/ }));
    await user.type(screen.getByLabelText(/Label/), "My tunnel");
    await user.click(screen.getByRole("button", { name: "Create tunnel" }));
    expect(setupAsync).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: "nord",
        body: expect.objectContaining({
          country: "us",
          server_id: "s1",
          label: "My tunnel",
        }),
      }),
    );
    expect(onSuccess).toHaveBeenCalled();
  });
});
