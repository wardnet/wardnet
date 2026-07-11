import { useCallback, useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { ArrowDown, ArrowUp, Lock } from "lucide-react";
import { Button } from "@wardnet/web";
import { FormActions } from "@wardnet/web";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@wardnet/web";
import { Field } from "@wardnet/web";
import { Form, Validator } from "@wardnet/web";
import { Input } from "@wardnet/web";
import { Pill } from "@wardnet/web";
import { Text } from "@wardnet/web";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wardnet/web";
import { DataTable, RowAction } from "@/components/core/ui/data-table";
import type {
  DnsProtocol,
  ForwarderSelectionMode,
  UpstreamDns,
  UpstreamLatency,
} from "@wardnet/js";

const PROTOCOLS: DnsProtocol[] = ["udp", "tcp", "tls", "https"];

/** Routing modes in display order, each with a label and a one-line
 *  behaviour description shown under the dropdown. */
const MODE_ORDER: ForwarderSelectionMode[] = ["failover", "fastest", "single"];
const MODE_META: Record<
  ForwarderSelectionMode,
  { label: string; description: string }
> = {
  failover: {
    label: "Failover (in order)",
    description:
      "Queries go to the first server below; WARDNET only falls back to the next if it stops responding. Use the up/down actions to set the priority order.",
  },
  fastest: {
    label: "Fastest response",
    description:
      "Queries go to whichever server is responding fastest right now (measured continuously). The list order doesn't matter.",
  },
  single: {
    label: "Single server",
    description:
      "Every query goes to one server only — choose it with the radio in the table below. The others are never used.",
  },
};

/** Format a probed latency for the table: rounded ms, or an em-dash when
 *  the prober hasn't produced a sample yet. */
function formatLatency(entry: UpstreamLatency | undefined): string {
  if (!entry || entry.avg_latency_ms == null) return "—";
  return `${Math.round(entry.avg_latency_ms)} ms`;
}

interface UpstreamServersCardProps {
  servers: UpstreamDns[];
  isSaving: boolean;
  /** When true (recursive mode), these upstreams are used only as a
   *  fallback if recursive resolution fails — surfaced as a note. */
  fallbackOnly?: boolean;
  /** Rolling-average latency per server (from GET /api/dns/status). */
  latencies?: UpstreamLatency[];
  /** Current routing mode. */
  mode: ForwarderSelectionMode;
  /** Address of the chosen server when `mode === "single"`. */
  selectedUpstream?: string;
  onUpdate: (servers: UpstreamDns[]) => void;
  /** Switch to a mode that uses all servers (failover / fastest). */
  onModeChange: (mode: ForwarderSelectionMode) => void;
  /** Switch to single-server mode using the upstream at `address`. */
  onSelectServer: (address: string) => void;
}

/** Upstream DNS servers — list/add/reorder/remove with the standard
 *  Forge data-table + row-actions pattern. A routing-mode dropdown above
 *  the table controls how the servers are used (failover / fastest /
 *  single). Adding is an inline form that appears above the table. */
export function UpstreamServersCard({
  servers,
  isSaving,
  fallbackOnly = false,
  latencies = [],
  mode,
  selectedUpstream,
  onUpdate,
  onModeChange,
  onSelectServer,
}: UpstreamServersCardProps) {
  const [adding, setAdding] = useState(false);

  // Optimistic routing selection so the dropdown/radios reflect a change
  // instantly rather than reverting while the PUT round-trips (the props only
  // update after the mutation's refetch lands). Without this the controls
  // flicker on every change.
  //
  // The override is *derived* out once the server props catch up (no effect,
  // no setState-in-effect): while it still disagrees with the props it wins;
  // once they match, we fall through to the props and the stale state is
  // simply ignored until the next change overwrites it.
  const [optimistic, setOptimistic] = useState<{
    mode: ForwarderSelectionMode;
    selected?: string;
  } | null>(null);
  const optimisticPending =
    optimistic !== null &&
    (optimistic.mode !== mode ||
      (optimistic.selected ?? undefined) !== (selectedUpstream ?? undefined));
  const effMode = optimistic && optimisticPending ? optimistic.mode : mode;
  const effSelected =
    optimistic && optimisticPending ? optimistic.selected : selectedUpstream;

  // Selecting a specific server (single-server mode).
  const handleSelectServer = useCallback(
    (address: string) => {
      setOptimistic({ mode: "single", selected: address });
      onSelectServer(address);
    },
    [onSelectServer],
  );

  // Changing the routing mode from the dropdown. Switching to single-server
  // needs a target server — keep the current one if still valid, else the
  // first in the list.
  const handleModeChange = useCallback(
    (next: ForwarderSelectionMode) => {
      if (next === "single") {
        const target =
          effSelected && servers.some((s) => s.address === effSelected)
            ? effSelected
            : servers[0]?.address;
        if (!target) return; // no servers to pin to
        setOptimistic({ mode: "single", selected: target });
        onSelectServer(target);
        return;
      }
      setOptimistic({ mode: next });
      onModeChange(next);
    },
    [effSelected, servers, onModeChange, onSelectServer],
  );

  const latencyByAddress = useMemo(() => {
    const map = new Map<string, UpstreamLatency>();
    for (const l of latencies) map.set(l.address, l);
    return map;
  }, [latencies]);

  function isSelected(address: string) {
    return effMode === "single" && effSelected === address;
  }

  function move(index: number, delta: number) {
    const next = [...servers];
    const target = index + delta;
    if (target < 0 || target >= next.length) return;
    // eslint-disable-next-line security/detect-object-injection -- row-reorder swap on a local array copy; index from the servers table row, target bounds-checked
    [next[index], next[target]] = [next[target], next[index]];
    onUpdate(next);
  }

  function remove(index: number) {
    onUpdate(servers.filter((_, i) => i !== index));
  }

  function add(server: UpstreamDns) {
    onUpdate([...servers, server]);
    setAdding(false);
  }

  const columns = useMemo<
    ColumnDef<UpstreamDns & { __index: number }>[]
  >(() => {
    const cols: ColumnDef<UpstreamDns & { __index: number }>[] = [];
    // The per-server radio only appears in single-server mode — that's the
    // only mode where "which one" is a choice.
    if (effMode === "single") {
      cols.push({
        id: "select",
        header: "Use",
        meta: { className: "w-10" },
        cell: ({ row }) => (
          <input
            type="radio"
            name="upstream-single"
            className="h-4 w-4 cursor-pointer align-middle"
            aria-label={`Send all DNS only to ${row.original.name}`}
            data-testid="upstream-select"
            checked={effSelected === row.original.address}
            onChange={() => handleSelectServer(row.original.address)}
          />
        ),
      });
    }
    cols.push(
      {
        id: "name",
        header: "Name",
        cell: ({ row }) => (
          <Text as="span" size="sm" weight="medium">
            {row.original.name}
          </Text>
        ),
      },
      {
        id: "address",
        header: "Address",
        cell: ({ row }) => (
          <Text as="span" size="xs" className="font-mono">
            {row.original.address}
            {row.original.port ? `:${row.original.port}` : ""}
          </Text>
        ),
      },
      {
        id: "latency",
        header: "Latency",
        cell: ({ row }) => {
          const entry = latencyByAddress.get(row.original.address);
          if (entry && !entry.reachable) {
            return (
              <Text as="span" size="xs" className="text-warn">
                Unreachable
              </Text>
            );
          }
          return (
            <Text as="span" size="xs" className="text-ink-3">
              {formatLatency(entry)}
            </Text>
          );
        },
      },
      {
        id: "protocol",
        header: "Protocol",
        // Hidden on mobile — the pill pushes the table wider than
        // the viewport. Reappears at md+ where the row has room.
        meta: { className: "hidden md:table-cell" },
        cell: ({ row }) => (
          <Pill variant="ghost">{row.original.protocol.toUpperCase()}</Pill>
        ),
      },
    );
    return cols;
    // Deps: the effective (optimistic) selection + memoized latency map + the
    // stable select handler. isSaving is deliberately excluded so a save
    // doesn't rebuild the table (that was a flicker source).
  }, [effMode, effSelected, latencyByAddress, handleSelectServer]);

  const rows = useMemo(
    () => servers.map((s, i) => ({ ...s, __index: i })),
    [servers],
  );

  return (
    <Card>
      <CardHeader>
        <CardTitle>Upstream servers</CardTitle>
        <CardAction>
          {!adding && (
            <Button
              variant="outline"
              data-testid="upstream-add"
              onClick={() => setAdding(true)}
              disabled={isSaving}
            >
              Add server
            </Button>
          )}
        </CardAction>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        {fallbackOnly && (
          <Text as="p" size="sm" className="text-ink-3">
            Recursive resolution is active — these upstreams are used only as a
            fallback if recursion fails. Leave empty to resolve purely from the
            root servers (failures return SERVFAIL instead of forwarding).
          </Text>
        )}
        {/* Routing mode: how the servers below are used. The selected mode's
            behaviour is described in a line underneath so the effect of each
            choice is explicit. */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <Text as="span" size="sm" weight="medium">
              Routing
            </Text>
            <Select
              value={effMode}
              onValueChange={(v) =>
                handleModeChange(v as ForwarderSelectionMode)
              }
            >
              <SelectTrigger className="w-56" data-testid="upstream-mode">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {MODE_ORDER.map((m) => (
                  <SelectItem
                    key={m}
                    value={m}
                    disabled={m === "single" && servers.length === 0}
                  >
                    {/* eslint-disable-next-line security/detect-object-injection -- m from .map() over the local const MODE_ORDER list */}
                    {MODE_META[m].label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <Text
            as="p"
            size="xs"
            className="text-ink-3"
            data-testid="upstream-mode-desc"
          >
            {/* eslint-disable-next-line security/detect-object-injection -- effMode is a closed ForwarderSelectionMode union key into the MODE_META const map */}
            {MODE_META[effMode].description}
          </Text>
        </div>

        {adding && (
          <AddServerForm
            onCancel={() => setAdding(false)}
            onSubmit={add}
            isSaving={isSaving}
          />
        )}

        {servers.length === 0 ? (
          <div className="empty">No upstream servers configured.</div>
        ) : (
          <DataTable
            columns={columns}
            data={rows}
            rowActionsTestId="upstream-row-menu"
            rowActions={(row) => (
              <>
                {/* Reordering only matters in failover mode, so only offer it
                    there — keeps the menu honest about what does anything. */}
                {effMode === "failover" && (
                  <>
                    <RowAction
                      onSelect={() => move(row.__index, -1)}
                      icon={<ArrowUp aria-hidden />}
                    >
                      Move up
                    </RowAction>
                    <RowAction
                      onSelect={() => move(row.__index, 1)}
                      icon={<ArrowDown aria-hidden />}
                    >
                      Move down
                    </RowAction>
                  </>
                )}
                {isSelected(row.address) ? (
                  // The chosen single server can't be removed — pick another
                  // first. The daemon rejects removing it (400); this makes
                  // that clear without a failed request.
                  <RowAction icon={<Lock aria-hidden />}>
                    Selected — pick another to remove
                  </RowAction>
                ) : (
                  <RowAction
                    onSelect={() => remove(row.__index)}
                    testId="upstream-remove"
                    destructive
                  >
                    Remove
                  </RowAction>
                )}
              </>
            )}
          />
        )}
      </CardContent>
    </Card>
  );
}

interface AddServerFormProps {
  onCancel: () => void;
  onSubmit: (server: UpstreamDns) => void;
  isSaving: boolean;
}

function AddServerForm({ onCancel, onSubmit, isSaving }: AddServerFormProps) {
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [port, setPort] = useState("");
  const [protocol, setProtocol] = useState<DnsProtocol>("udp");
  const [tlsServerName, setTlsServerName] = useState("");

  // DoT/DoH require a TLS server name (SNI) for certificate validation.
  const isEncrypted = protocol === "tls" || protocol === "https";

  function handleSave(values: {
    name: string;
    address: string;
    port: string;
    tls_server_name?: string;
  }) {
    const portNum = values.port ? Number(values.port) : undefined;
    const sni = values.tls_server_name?.trim();
    onSubmit({
      name: values.name.trim(),
      address: values.address.trim(),
      protocol,
      port: portNum && portNum > 0 ? portNum : undefined,
      tls_server_name: isEncrypted && sni ? sni : undefined,
    });
  }

  return (
    <Card className="border-dashed">
      <CardHeader>
        <CardTitle>Add upstream server</CardTitle>
      </CardHeader>
      <Form
        values={{ name, address, port, tls_server_name: tlsServerName }}
        onSubmit={handleSave}
      >
        <CardContent className="flex flex-col gap-5">
          <div className="flex gap-3">
            <Field
              label="Name"
              htmlFor="ups-name"
              name="name"
              className="flex-1"
            >
              <Input
                id="ups-name"
                data-testid="upstream-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Cloudflare"
              />
            </Field>
            <Validator
              name="name"
              rule="required"
              message="Name is required."
            />

            <Field label="Protocol" htmlFor="ups-proto" className="flex-1">
              <Select
                value={protocol}
                onValueChange={(v) => setProtocol(v as DnsProtocol)}
              >
                <SelectTrigger id="ups-proto" data-testid="upstream-protocol">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {PROTOCOLS.map((p) => (
                    <SelectItem key={p} value={p}>
                      {p.toUpperCase()}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </Field>
          </div>

          <div className="flex gap-3">
            <Field
              label="Address"
              htmlFor="ups-addr"
              name="address"
              className="flex-1"
            >
              <Input
                id="ups-addr"
                data-testid="upstream-address"
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                placeholder="1.1.1.1"
              />
            </Field>
            <Validator
              name="address"
              rule="required"
              message="Address is required."
            />

            <Field
              label="Port (optional)"
              htmlFor="ups-port"
              name="port"
              className="flex-1"
            >
              <Input
                id="ups-port"
                type="number"
                value={port}
                onChange={(e) => setPort(e.target.value)}
                placeholder="853"
              />
            </Field>
          </div>

          {isEncrypted && (
            <>
              <Field
                label="TLS server name"
                htmlFor="ups-sni"
                name="tls_server_name"
                help="Hostname the upstream's certificate must match (e.g. cloudflare-dns.com)."
              >
                <Input
                  id="ups-sni"
                  value={tlsServerName}
                  onChange={(e) => setTlsServerName(e.target.value)}
                  placeholder="cloudflare-dns.com"
                />
              </Field>
              <Validator
                name="tls_server_name"
                rule="required"
                message="TLS server name is required for DoT/DoH."
              />
            </>
          )}
        </CardContent>
        <FormActions
          secondaryLabel="Cancel"
          secondaryProps={{
            type: "button",
            onClick: onCancel,
            disabled: isSaving,
          }}
          primaryLabel="Add server"
          primaryProps={{
            type: "submit",
            "data-testid": "upstream-submit",
            disabled: isSaving,
          }}
        />
      </Form>
    </Card>
  );
}
