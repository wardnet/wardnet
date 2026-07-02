import { useState } from "react";
import { Button } from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Input } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { CountryCombobox } from "@/components/compound/CountryCombobox";
import { ApiErrorAlert } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "@wardnet/web";
import { countryFlag } from "@wardnet/web";
import type {
  ProviderCredentials,
  ProviderInfo,
  ServerInfo,
} from "@wardnet/js";

/** Visual load indicator with a colored dot and percentage. */
function LoadIndicator({ load }: { load: number }) {
  const color =
    load < 30
      ? "text-accent-ink"
      : load < 70
        ? "text-warn-soft-ink"
        : "text-danger";
  return (
    <Text as="span" size="xs" className={`flex items-center gap-1 ${color}`}>
      <span className="inline-block size-2 rounded-full bg-current" />
      {load}%
    </Text>
  );
}

/** Inline provider logo or letter fallback. */
function ProviderLogo({ provider }: { provider: ProviderInfo | undefined }) {
  if (!provider) return null;
  if (provider.icon_url) {
    return (
      <img
        src={provider.icon_url}
        alt=""
        className="size-4 rounded-sm object-contain"
      />
    );
  }
  return (
    <Text
      as="span"
      size="2xs"
      weight="bold"
      className="flex size-4 items-center justify-center rounded-sm bg-sunken uppercase text-ink-3"
    >
      {provider.name[0]}
    </Text>
  );
}

/** Server list item with load indicator. */
function ServerOption({ server }: { server: ServerInfo }) {
  const flag = server.country_code ? countryFlag(server.country_code) : "";
  return (
    <span className="flex w-full items-center justify-between gap-2">
      <span>
        {flag && <span className="mr-1">{flag}</span>}
        {server.name}
      </span>
      {server.load != null && <LoadIndicator load={server.load} />}
    </span>
  );
}

interface ProviderTunnelTabProps {
  onSuccess: () => void;
}

/** Provider-based tunnel creation with credential validation and server selection. */
export function ProviderTunnelTab({ onSuccess }: ProviderTunnelTabProps) {
  const { data: providerData } = useProviders();
  const providers = providerData?.providers ?? [];

  const validateCreds = useValidateCredentials();
  const fetchServers = useProviderServers();
  const setupTunnel = useProviderSetup();

  const [providerId, setProviderId] = useState("");
  const [authMethod, setAuthMethod] = useState<"credentials" | "token">(
    "credentials",
  );
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [token, setToken] = useState("");
  const [credsValid, setCredsValid] = useState(false);
  const [country, setCountry] = useState("");
  const [serverId, setServerId] = useState("");
  const [hostname, setHostname] = useState("");
  const [labelOverride, setLabelOverride] = useState("");

  const selectedProvider = providers.find((p) => p.id === providerId);
  const { data: countryData } = useProviderCountries(providerId);
  const countries = countryData?.countries ?? [];
  const servers = fetchServers.data?.servers ?? [];
  const supportsToken =
    selectedProvider?.auth_methods.includes("token") ?? false;
  const supportsCreds =
    selectedProvider?.auth_methods.includes("credentials") ?? false;

  function credentials(): ProviderCredentials {
    if (authMethod === "token") {
      return { type: "token", token };
    }
    return { type: "credentials", username, password };
  }

  const credsReady =
    authMethod === "token"
      ? token.length > 0
      : username.length > 0 && password.length > 0;

  async function handleValidate() {
    const result = await validateCreds.mutateAsync({
      providerId,
      body: { credentials: credentials() },
    });
    setCredsValid(result.valid);
    if (result.valid) {
      fetchServers.mutate({
        providerId,
        body: {
          credentials: credentials(),
          filter: country ? { country } : {},
        },
      });
    }
  }

  async function handleFetchServers() {
    fetchServers.mutate({
      providerId,
      body: { credentials: credentials(), filter: country ? { country } : {} },
    });
  }

  async function handleSetup() {
    await setupTunnel.mutateAsync({
      providerId,
      body: {
        credentials: credentials(),
        country: country || undefined,
        label: labelOverride || undefined,
        server_id: serverId || undefined,
        hostname: hostname || undefined,
      },
    });
    onSuccess();
  }

  return (
    <div className="flex flex-col gap-4">
      {/* Step 1: Select provider */}
      <Field label="Provider">
        {providers.length === 0 ? (
          <Text as="p" size="sm" className="text-ink-3">
            No providers available.
          </Text>
        ) : (
          <Select
            value={providerId}
            onValueChange={(v) => {
              setProviderId(v);
              setCredsValid(false);
              const prov = providers.find((p) => p.id === v);
              setAuthMethod(
                (prov?.auth_methods[0] as "token" | "credentials") ??
                  "credentials",
              );
            }}
          >
            <SelectTrigger className="w-full" data-testid="provider-select">
              <SelectValue placeholder="Select a provider" />
            </SelectTrigger>
            <SelectContent>
              {providers.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  <span className="flex items-center gap-2">
                    <ProviderLogo provider={p} />
                    {p.name}
                  </span>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </Field>

      {/* Step 2: Credentials */}
      {selectedProvider && (
        <>
          {supportsToken && supportsCreds && (
            <Field label="Auth method">
              <Select
                value={authMethod}
                onValueChange={(v) => {
                  setAuthMethod(v as "credentials" | "token");
                  setCredsValid(false);
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="token">Access token</SelectItem>
                  <SelectItem value="credentials">
                    Username &amp; password
                  </SelectItem>
                </SelectContent>
              </Select>
            </Field>
          )}

          {authMethod === "token" ? (
            <Field
              label="Access token"
              htmlFor="prov-token"
              help={selectedProvider?.credentials_hint}
            >
              <Input
                id="prov-token"
                data-testid="provider-token"
                type="password"
                value={token}
                onChange={(e) => {
                  setToken(e.target.value);
                  setCredsValid(false);
                }}
                placeholder="Paste your access token"
              />
            </Field>
          ) : (
            <>
              <Field label="Username" htmlFor="prov-username">
                <Input
                  id="prov-username"
                  value={username}
                  onChange={(e) => {
                    setUsername(e.target.value);
                    setCredsValid(false);
                  }}
                  placeholder="Service username"
                />
              </Field>
              <Field label="Password" htmlFor="prov-password">
                <Input
                  id="prov-password"
                  type="password"
                  value={password}
                  onChange={(e) => {
                    setPassword(e.target.value);
                    setCredsValid(false);
                  }}
                  placeholder="Service password"
                />
              </Field>
            </>
          )}
          <div className="flex items-center justify-end gap-3">
            {validateCreds.data && !validateCreds.data.valid && (
              <Text as="p" size="sm" className="flex-1 text-danger">
                {validateCreds.data.message}
              </Text>
            )}
            <Button
              variant="secondary"
              data-testid="provider-validate"
              onClick={handleValidate}
              disabled={!credsReady || validateCreds.isPending}
            >
              {validateCreds.isPending ? "Validating…" : "Validate credentials"}
            </Button>
          </div>
          {validateCreds.isError && (
            <ApiErrorAlert
              error={validateCreds.error}
              fallback="Validation failed"
            />
          )}
        </>
      )}

      {/* Step 3: Country + server selection */}
      {credsValid && (
        <>
          <div className="flex items-end gap-3">
            <Field label="Country" className="flex-1">
              {countries.length > 0 ? (
                <CountryCombobox
                  countries={countries}
                  value={country}
                  onChange={(v) => {
                    setCountry(v);
                    setServerId("");
                    fetchServers.mutate({
                      providerId,
                      body: {
                        credentials: credentials(),
                        filter: v ? { country: v } : {},
                      },
                    });
                  }}
                />
              ) : (
                <Input
                  value={country}
                  onChange={(e) => setCountry(e.target.value)}
                  placeholder="Country code (optional)"
                  maxLength={2}
                />
              )}
            </Field>
            <Button
              variant="secondary"
              onClick={handleFetchServers}
              disabled={fetchServers.isPending}
            >
              {fetchServers.isPending ? "Loading…" : "Refresh"}
            </Button>
          </div>

          <Field
            label="Hostname (optional)"
            htmlFor="prov-hostname"
            help="For dedicated IP or manual server selection. Overrides server list selection."
          >
            <Input
              id="prov-hostname"
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              placeholder="e.g. pt131 or pt131.nordvpn.com"
            />
          </Field>

          {country && servers.length > 0 && !hostname && (
            <Field label="Server">
              <Select value={serverId} onValueChange={setServerId}>
                <SelectTrigger className="w-full">
                  <SelectValue
                    placeholder={`Auto-select best server in ${country.toUpperCase()}`}
                  />
                </SelectTrigger>
                <SelectContent>
                  {servers.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      <ServerOption server={s} />
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          )}

          <Field label="Label (optional)" htmlFor="prov-label">
            <Input
              id="prov-label"
              data-testid="provider-label"
              value={labelOverride}
              onChange={(e) => setLabelOverride(e.target.value)}
              placeholder="Auto-generated from server"
            />
          </Field>

          {setupTunnel.isError && (
            <ApiErrorAlert error={setupTunnel.error} fallback="Setup failed" />
          )}

          <div className="flex justify-end">
            <Button
              data-testid="provider-setup-submit"
              onClick={handleSetup}
              disabled={setupTunnel.isPending}
            >
              {setupTunnel.isPending ? "Setting up…" : "Create tunnel"}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
