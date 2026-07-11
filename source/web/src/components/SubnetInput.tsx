import { useState } from "react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Text,
} from "@wardnet/ui";
import { Ipv4Input } from "./Ipv4Input";
import {
  buildCidr,
  isPrivateCidr,
  octetsValid,
  parseCidr,
  usableHosts,
} from "../lib/cidr";

/**
 * Sizes offered by the picker, largest prefix (smallest net) first. Covers
 * /30 (2 hosts) up to /16 (65k) — anything bigger than a /16 for a single LAN
 * zone is never sensible, and a direct API call is still range-validated by the
 * daemon.
 */
const SIZE_OPTIONS: { hosts: number; prefix: number }[] = Array.from(
  { length: 15 },
  (_, i) => {
    const prefix = 30 - i; // 30 → 16
    return { hosts: usableHosts(prefix), prefix };
  },
);

interface SubnetInputProps {
  /** Current value as a normalized network CIDR (e.g. "10.44.0.0/26"), or "". */
  value: string;
  /** Called with the resolved CIDR, or "" while the inputs are incomplete or
   *  the range is not fully RFC 1918 private. */
  onChange: (cidr: string) => void;
  id?: string;
  testId?: string;
}

/**
 * How many trailing octets are *entirely* host bits for a prefix — these are
 * locked to 0 in the UI. The boundary octet (partly network) stays editable.
 */
function lockedOctets(prefix: number): number {
  return Math.floor((32 - prefix) / 8);
}

/**
 * Resolve the network octets for a base address + prefix. The leading, fully
 * within-prefix octets must be filled; the boundary + host octets default to 0
 * (the operator can still set the boundary octet to pick, e.g., which /26).
 * Returns null while a required octet is missing or invalid.
 */
function resolveOctets(
  ip: string,
  prefix: number,
): [number, number, number, number] | null {
  const parts = ip.split(".");
  const required = Math.floor(prefix / 8); // leading fully-network octets
  const out: number[] = [];
  for (let i = 0; i < 4; i++) {
    // eslint-disable-next-line security/detect-object-injection -- i is a bounded 0-3 loop counter reading a locally split octet array
    const raw = parts[i];
    if (i < required) {
      if (raw === undefined || raw === "") return null;
      out.push(Number(raw));
    } else {
      out.push(raw === undefined || raw === "" ? 0 : Number(raw));
    }
  }
  return octetsValid(out) ? (out as [number, number, number, number]) : null;
}

/**
 * Friendly IPv4 subnet picker, reused wherever a CIDR is needed. A base address
 * plus a size dropdown that sets the prefix; the fully-host octets are locked to
 * 0, the result is shown as a normalized network CIDR with its usable-host
 * count, and anything not fully inside an RFC 1918 private block is rejected.
 */
export function SubnetInput({ value, onChange, id, testId }: SubnetInputProps) {
  const parsed = parseCidr(value);
  const [baseIp, setBaseIp] = useState<string>(
    parsed ? parsed.octets.join(".") : "",
  );
  const [prefix, setPrefix] = useState<number>(parsed ? parsed.prefix : 24);

  const t = testId ?? "subnet";
  const octets = resolveOctets(baseIp, prefix);
  const resolved = octets ? buildCidr(octets, prefix) : "";
  const isPrivate = resolved !== "" && isPrivateCidr(resolved);

  function emit(ip: string, nextPrefix: number) {
    const n = resolveOctets(ip, nextPrefix);
    const cidr = n ? buildCidr(n, nextPrefix) : "";
    onChange(cidr && isPrivateCidr(cidr) ? cidr : "");
  }

  function setBase(ip: string) {
    setBaseIp(ip);
    emit(ip, prefix);
  }

  function setPrefixValue(raw: string) {
    const next = Number(raw);
    setPrefix(next);
    emit(baseIp, next);
  }

  return (
    <div className="flex flex-col gap-2" id={id}>
      <div className="flex flex-wrap items-center gap-2">
        <Ipv4Input
          value={baseIp}
          onChange={setBase}
          readOnlyOctets={lockedOctets(prefix)}
          data-testid={`${t}-ip`}
          className="w-fit"
        />
        <Select value={String(prefix)} onValueChange={setPrefixValue}>
          <SelectTrigger
            data-testid={`${t}-size`}
            aria-label="Subnet size"
            className="w-56"
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {SIZE_OPTIONS.map((o) => (
              <SelectItem key={o.prefix} value={String(o.prefix)}>
                Up to {o.hosts.toLocaleString()} devices (/{o.prefix})
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {resolved && !isPrivate ? (
        <Text size="xs" className="text-danger" data-testid={`${t}-cidr`}>
          Must be a private range (10.x, 172.16-31.x, or 192.168.x).
        </Text>
      ) : (
        <Text size="xs" className="text-ink-3" data-testid={`${t}-cidr`}>
          {resolved
            ? `Resolves to ${resolved} - ${usableHosts(prefix)} usable hosts`
            : "Enter a base address to build the subnet."}
        </Text>
      )}
    </div>
  );
}
