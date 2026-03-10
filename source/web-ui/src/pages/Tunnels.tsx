import { useState } from "react";
import { Button } from "@/components/core/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/core/ui/card";
import { Badge } from "@/components/core/ui/badge";
import { Input } from "@/components/core/ui/input";
import { Label } from "@/components/core/ui/label";
import { Textarea } from "@/components/core/ui/textarea";
import { Sheet, SheetContent, SheetTitle, SheetTrigger } from "@/components/core/ui/sheet";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/core/ui/tabs";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";
import { PageHeader } from "@/components/compound/PageHeader";
import { CountryCombobox } from "@/components/compound/CountryCombobox";
import { ApiErrorAlert } from "@/components/compound/ApiErrorAlert";
import { useTunnels, useCreateTunnel, useDeleteTunnel } from "@/hooks/useTunnels";
import {
  useProviders,
  useProviderCountries,
  useValidateCredentials,
  useProviderServers,
  useProviderSetup,
} from "@/hooks/useProviders";
import { formatBytes, timeAgo } from "@/lib/utils";
import { countryFlag } from "@/lib/country";
import type {
  Tunnel,
  TunnelStatus,
  ProviderCredentials,
  ProviderInfo,
  ServerInfo,
} from "@wardnet/js";

function statusColor(status: TunnelStatus) {
  const map = {
    up: "default",
    down: "secondary",
    connecting: "outline",
  } as const;
  return map[status];
}

function statusLabel(status: TunnelStatus): string {
  switch (status) {
    case "up":
      return "Active";
    case "down":
      return "Down";
    case "connecting":
      return "Connecting";
  }
}

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

function TunnelCard({
  tunnel,
  providers,
  onDelete,
}: {
  tunnel: Tunnel;
  providers: ProviderInfo[];
  onDelete: (id: string) => void;
}) {
  const provider = providers.find((p) => p.id === tunnel.provider);
  const flag = tunnel.country_code ? countryFlag(tunnel.country_code) : "";

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <div className="flex items-center gap-3">
          {provider && <ProviderLogo provider={provider} />}
          <div className="flex flex-col gap-1">
            <CardTitle className="text-base">
              {flag && <span className="mr-1.5">{flag}</span>}
              {tunnel.label}
            </CardTitle>
            <p className="text-xs text-muted-foreground">
              {tunnel.country_code && tunnel.country_code.toUpperCase()}
              {provider && ` · ${provider.name}`}
              {!provider && tunnel.provider && ` · ${tunnel.provider}`}
            </p>
          </div>
        </div>
        <Badge variant={statusColor(tunnel.status)}>{statusLabel(tunnel.status)}</Badge>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-2 gap-y-2 text-sm">
          <div>
            <span className="text-muted-foreground">Interface</span>
            <p className="font-mono text-xs">{tunnel.interface_name}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Endpoint</span>
            <p className="font-mono text-xs">{tunnel.endpoint}</p>
          </div>
          <div>
            <span className="text-muted-foreground">Traffic</span>
            <p className="text-xs">
              {formatBytes(tunnel.bytes_tx)} up / {formatBytes(tunnel.bytes_rx)} down
            </p>
          </div>
          <div>
            <span className="text-muted-foreground">Last handshake</span>
            <p className="text-xs">
              {tunnel.last_handshake ? timeAgo(tunnel.last_handshake) : "—"}
            </p>
          </div>
        </div>
        <div className="mt-4 flex justify-end">
          <Button variant="destructive" size="sm" onClick={() => onDelete(tunnel.id)}>
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function ManualTab({ onSuccess }: { onSuccess: () => void }) {
  const [label, setLabel] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const [provider, setProvider] = useState("");
  const [config, setConfig] = useState("");
  const createTunnel = useCreateTunnel();

  function reset() {
    setLabel("");
    setCountryCode("");
    setProvider("");
    setConfig("");
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    await createTunnel.mutateAsync({
      label,
      country_code: countryCode,
      provider: provider || undefined,
      config,
    });
    reset();
    onSuccess();
  }

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <Label htmlFor="tunnel-label">Label</Label>
        <Input
          id="tunnel-label"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="US West"
          required
        />
      </div>
      <div className="flex gap-3">
        <div className="flex flex-1 flex-col gap-2">
          <Label htmlFor="tunnel-country">Country Code</Label>
          <Input
            id="tunnel-country"
            value={countryCode}
            onChange={(e) => setCountryCode(e.target.value)}
            placeholder="US"
            maxLength={2}
            required
          />
        </div>
        <div className="flex flex-1 flex-col gap-2">
          <Label htmlFor="tunnel-provider">Provider</Label>
          <Input
            id="tunnel-provider"
            value={provider}
            onChange={(e) => setProvider(e.target.value)}
            placeholder="Mullvad"
          />
        </div>
      </div>
      <div className="flex flex-col gap-2">
        <Label htmlFor="tunnel-config">WireGuard Config</Label>
        <Textarea
          id="tunnel-config"
          value={config}
          onChange={(e) => setConfig(e.target.value)}
          placeholder="Paste your .conf file contents here..."
          required
          rows={10}
          className="font-mono"
        />
      </div>
      {createTunnel.isError && (
        <ApiErrorAlert error={createTunnel.error} fallback="Failed to create tunnel" />
      )}
      <Button type="submit" disabled={createTunnel.isPending} className="w-full">
        {createTunnel.isPending ? "Creating..." : "Create Tunnel"}
      </Button>
    </form>
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

function ProviderTab({ onSuccess }: { onSuccess: () => void }) {
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

function CreateTunnelSheet() {
  const [open, setOpen] = useState(false);

  return (
    <Sheet open={open} onOpenChange={setOpen}>
      <SheetTrigger asChild>
        <Button>Add Tunnel</Button>
      </SheetTrigger>
      <SheetContent className="w-full overflow-y-auto p-6">
        <SheetTitle>Add WireGuard Tunnel</SheetTitle>
        <Tabs defaultValue="manual" className="mt-6">
          <TabsList className="w-full">
            <TabsTrigger value="manual" className="flex-1">
              Manual
            </TabsTrigger>
            <TabsTrigger value="provider" className="flex-1">
              Provider
            </TabsTrigger>
          </TabsList>
          <TabsContent value="manual" className="mt-4">
            <ManualTab onSuccess={() => setOpen(false)} />
          </TabsContent>
          <TabsContent value="provider" className="mt-4">
            <ProviderTab onSuccess={() => setOpen(false)} />
          </TabsContent>
        </Tabs>
      </SheetContent>
    </Sheet>
  );
}

/** Tunnels page for managing WireGuard VPN tunnels (admin only). */
export default function Tunnels() {
  const { data, isLoading, isError } = useTunnels();
  const { data: providerData } = useProviders();
  const deleteTunnel = useDeleteTunnel();
  const tunnels = data?.tunnels ?? [];
  const providers = providerData?.providers ?? [];

  return (
    <>
      <PageHeader title="Tunnels" actions={<CreateTunnelSheet />} />

      {isLoading && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Loading tunnels...
          </CardContent>
        </Card>
      )}

      {isError && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            Failed to load tunnels. Make sure the daemon is running.
          </CardContent>
        </Card>
      )}

      {!isLoading && !isError && tunnels.length === 0 && (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            No tunnels configured. Add a WireGuard tunnel to get started.
          </CardContent>
        </Card>
      )}

      {tunnels.length > 0 && (
        <div className="grid gap-4 md:grid-cols-2">
          {tunnels.map((tunnel) => (
            <TunnelCard
              key={tunnel.id}
              tunnel={tunnel}
              providers={providers}
              onDelete={(id) => deleteTunnel.mutate(id)}
            />
          ))}
        </div>
      )}
    </>
  );
}
