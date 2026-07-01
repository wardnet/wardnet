import { useMemo, useState } from "react";
import type { ColumnDef } from "@tanstack/react-table";
import { ArrowDown, ArrowUp } from "lucide-react";
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
import type { DnsProtocol, UpstreamDns } from "@wardnet/js";

const PROTOCOLS: DnsProtocol[] = ["udp", "tcp", "tls", "https"];

interface UpstreamServersCardProps {
  servers: UpstreamDns[];
  isSaving: boolean;
  /** When true (recursive mode), these upstreams are used only as a
   *  fallback if recursive resolution fails — surfaced as a note. */
  fallbackOnly?: boolean;
  onUpdate: (servers: UpstreamDns[]) => void;
}

/** Upstream DNS servers — list/add/reorder/remove with the standard
 *  Forge data-table + row-actions pattern. Adding is an inline form
 *  that appears above the table (like CreateReservationInline). */
export function UpstreamServersCard({
  servers,
  isSaving,
  fallbackOnly = false,
  onUpdate,
}: UpstreamServersCardProps) {
  const [adding, setAdding] = useState(false);

  function move(index: number, delta: number) {
    const next = [...servers];
    const target = index + delta;
    if (target < 0 || target >= next.length) return;
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

  const columns = useMemo<ColumnDef<UpstreamDns & { __index: number }>[]>(
    () => [
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
        id: "protocol",
        header: "Protocol",
        // Hidden on mobile — the pill pushes the table wider than
        // the viewport. Reappears at md+ where the row has room.
        meta: { className: "hidden md:table-cell" },
        cell: ({ row }) => (
          <Pill variant="ghost">{row.original.protocol.toUpperCase()}</Pill>
        ),
      },
    ],
    [],
  );

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
                <RowAction
                  onSelect={() => remove(row.__index)}
                  testId="upstream-remove"
                  destructive
                >
                  Remove
                </RowAction>
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
                <SelectTrigger id="ups-proto">
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
