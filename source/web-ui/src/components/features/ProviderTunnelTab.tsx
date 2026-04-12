import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { CountryCombobox } from "@/components/compound/CountryCombobox";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "@/hooks/useProviders";
import { countryFlag } from "@/lib/country";
import type { ProviderCredentials, ProviderInfo, ServerInfo } from "@wardnet/js";

/** Visual load indicator with a colored dot and percentage. */
function LoadIndicator({ load }: { load: number }) {
  const color = load < 30 ? "text-green-500" : load < 70 ? "text-yellow-500" : "text-red-500";
  return (
    <span className={`flex items-center gap-1 text-xs ${color}`}>
      <span className="inline-block size-2 rounded-full bg-current" />
      {load}%
    </span>
  );
}

/** Inline provider logo or letter fallback. */
function ProviderLogo({ provider }: { provider: ProviderInfo | undefined }) {
  if (!provider) return null;
  if (provider.icon_url) {
    return <img src={provider.icon_url} alt="" className="size-4 rounded-sm object-contain" />;
  }
  return (
    <span className="flex size-4 items-center justify-center rounded-sm bg-muted text-[10px] font-bold uppercase text-muted-foreground">
      {provider.name[0]}
    </span>
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
  const [authMethod, setAuthMethod] = useState<"credentials" | "token">("credentials");
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
  const supportsToken = selectedProvider?.auth_methods.includes("token") ?? false;
  const supportsCreds = selectedProvider?.auth_methods.includes("credentials") ?? false;

  function credentials(): ProviderCredentials {
    if (authMethod === "token") {
      return { type: "token", token };
    }
    return { type: "credentials", username, password };
  }

  const credsReady =
    authMethod === "token" ? token.length > 0 : username.length > 0 && password.length > 0;

  async function handleValidate() {
    const result = await validateCreds.mutateAsync({
      providerId,
      body: { credentials: credentials() },
    });
    setCredsValid(result.valid);
    if (result.valid) {
      fetchServers.mutate({
        providerId,
        body: { credentials: credentials(), filter: country ? { country } : {} },
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
      <div className="flex flex-col gap-2">
        <Label>Provider</Label>
        {providers.length === 0 ? (
          <p className="text-sm text-muted-foreground">No providers available.</p>
        ) : (
          <Select
            value={providerId}
            onValueChange={(v) => {
              setProviderId(v);
              setCredsValid(false);
              const prov = providers.find((p) => p.id === v);
              setAuthMethod((prov?.auth_methods[0] as "token" | "credentials") ?? "credentials");
            }}
          >
            <SelectTrigger className="w-full">
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
      </div>

      {/* Step 2: Credentials */}
      {selectedProvider && (
        <>
          {supportsToken && supportsCreds && (
            <div className="flex flex-col gap-2">
              <Label>Auth Method</Label>
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
                  <SelectItem value="token">Access Token</SelectItem>
                  <SelectItem value="credentials">Username & Password</SelectItem>
                </SelectContent>
              </Select>
            </div>
          )}

          {authMethod === "token" ? (
            <div className="flex flex-col gap-2">
              <Label htmlFor="prov-token">Access Token</Label>
              <Input
                id="prov-token"
                type="password"
                value={token}
                onChange={(e) => {
                  setToken(e.target.value);
                  setCredsValid(false);
                }}
                placeholder="Paste your access token"
              />
              {selectedProvider?.credentials_hint && (
                <p className="text-xs text-muted-foreground">{selectedProvider.credentials_hint}</p>
              )}
            </div>
          ) : (
            <>
              <div className="flex flex-col gap-2">
                <Label htmlFor="prov-username">Username</Label>
                <Input
                  id="prov-username"
                  value={username}
                  onChange={(e) => {
                    setUsername(e.target.value);
                    setCredsValid(false);
                  }}
                  placeholder="Service username"
                />
              </div>
              <div className="flex flex-col gap-2">
                <Label htmlFor="prov-password">Password</Label>
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
              </div>
            </>
          )}
          <Button
            variant="secondary"
            onClick={handleValidate}
            disabled={!credsReady || validateCreds.isPending}
          >
            {validateCreds.isPending ? "Validating..." : "Validate Credentials"}
          </Button>
          {validateCreds.isError && (
            <ApiErrorAlert error={validateCreds.error} fallback="Validation failed" />
          )}
          {validateCreds.data && !validateCreds.data.valid && (
            <p className="text-sm text-destructive">{validateCreds.data.message}</p>
          )}
        </>
      )}

      {/* Step 3: Country + server selection */}
      {credsValid && (
        <>
          <div className="flex items-end gap-3">
            <div className="flex flex-1 flex-col gap-2">
              <Label>Country</Label>
              {countries.length > 0 ? (
                <CountryCombobox
                  countries={countries}
                  value={country}
                  onChange={(v) => {
                    setCountry(v);
                    setServerId("");
                    fetchServers.mutate({
                      providerId,
                      body: { credentials: credentials(), filter: v ? { country: v } : {} },
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
            </div>
            <Button
              variant="secondary"
              onClick={handleFetchServers}
              disabled={fetchServers.isPending}
            >
              {fetchServers.isPending ? "Loading..." : "Refresh"}
            </Button>
          </div>

          <div className="flex flex-col gap-2">
            <Label htmlFor="prov-hostname">Hostname (optional)</Label>
            <Input
              id="prov-hostname"
              value={hostname}
              onChange={(e) => setHostname(e.target.value)}
              placeholder="e.g. pt131 or pt131.nordvpn.com"
            />
            <p className="text-xs text-muted-foreground">
              For dedicated IP or manual server selection. Overrides server list selection.
            </p>
          </div>

          {servers.length > 0 && !hostname && (
            <div className="flex flex-col gap-2">
              <Label>Server</Label>
              <Select value={serverId} onValueChange={setServerId}>
                <SelectTrigger className="w-full">
                  <SelectValue placeholder="Auto-select best server" />
                </SelectTrigger>
                <SelectContent>
                  {servers.map((s) => (
                    <SelectItem key={s.id} value={s.id}>
                      <ServerOption server={s} />
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          <div className="flex flex-col gap-2">
            <Label htmlFor="prov-label">Label (optional)</Label>
            <Input
              id="prov-label"
              value={labelOverride}
              onChange={(e) => setLabelOverride(e.target.value)}
              placeholder="Auto-generated from server"
            />
          </div>

          {setupTunnel.isError && (
            <ApiErrorAlert error={setupTunnel.error} fallback="Setup failed" />
          )}

          <Button onClick={handleSetup} disabled={setupTunnel.isPending} className="w-full">
            {setupTunnel.isPending ? "Setting up..." : "Create Tunnel"}
          </Button>
        </>
      )}
    </div>
  );
}
