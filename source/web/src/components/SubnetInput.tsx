import { useState } from "react";
import { SlidersHorizontal } from "lucide-react";
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

/** Common LAN sizes offered in simple mode — usable hosts → prefix. */
const SIZE_OPTIONS: { hosts: number; prefix: number }[] = [
  { hosts: 6, prefix: 29 },
  { hosts: 14, prefix: 28 },
  { hosts: 30, prefix: 27 },
  { hosts: 62, prefix: 26 },
  { hosts: 126, prefix: 25 },
  { hosts: 254, prefix: 24 },
  { hosts: 510, prefix: 23 },
  { hosts: 1022, prefix: 22 },
];

/** Prefix lengths offered in advanced mode (a private LAN never needs < /8). */
const ADVANCED_PREFIXES: number[] = Array.from(
  { length: 23 },
  (_, i) => 30 - i, // 30 → 8
);

interface SubnetInputProps {
  /** Current value as a normalized network CIDR (e.g. "10.44.0.0/26"), or "". */
  value: string;
  /** Called with the resolved CIDR, or "" while the inputs are incomplete or
   *  the address is not a private (RFC 1918) range. */
  onChange: (cidr: string) => void;
  id?: string;
  testId?: string;
}

type Mode = "simple" | "advanced";

/**
 * How many trailing octets are locked to `0`. Rounds **up** so any octet the
 * prefix forces to 0 is locked — e.g. a /26 keeps the whole last octet at 0
 * (the operator picks the /26 within a /24 via the network octets), and a /16
 * locks the last two.
 */
function lockedOctets(prefix: number): number {
  return Math.ceil((32 - prefix) / 8);
}

/**
 * Resolve the network octets for a base address + prefix. The host octets are
 * locked to `0` in the UI (the prefix decides them), so only the *network*
 * octets need to be filled — the rest default to `0`. Returns null while a
 * network octet is still missing or invalid.
 */
function resolveOctets(
  ip: string,
  prefix: number,
): [number, number, number, number] | null {
  const parts = ip.split(".");
  const networkCount = 4 - lockedOctets(prefix);
  const out: number[] = [];
  for (let i = 0; i < 4; i++) {
    if (i >= networkCount) {
      out.push(0);
      continue;
    }
    const raw = parts[i];
    if (raw === undefined || raw === "") return null;
    out.push(Number(raw));
  }
  return octetsValid(out) ? (out as [number, number, number, number]) : null;
}

/**
 * Friendly IPv4 subnet picker, reused wherever a CIDR is needed. Two modes over
 * one value:
 *  - **Simple**: a base address + a size dropdown that picks the prefix.
 *  - **Advanced**: a base address + an explicit prefix length.
 * The host portion of the address is locked to `0` (the prefix decides it), the
 * result is shown as a normalized network CIDR with its usable-host count, and
 * anything outside the RFC 1918 private ranges is rejected.
 */
export function SubnetInput({ value, onChange, id, testId }: SubnetInputProps) {
  const parsed = parseCidr(value);
  const [mode, setMode] = useState<Mode>("simple");
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

  function setPrefixValue(raw: number | string) {
    const n = Number(raw);
    const nextPrefix = Number.isFinite(n)
      ? Math.min(32, Math.max(0, n))
      : prefix;
    setPrefix(nextPrefix);
    emit(baseIp, nextPrefix);
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
        <button
          type="button"
          data-testid={`${t}-mode-toggle`}
          onClick={() => setMode(mode === "simple" ? "advanced" : "simple")}
          title={
            mode === "simple"
              ? "Switch to advanced (set the prefix directly)"
              : "Switch to simple (pick a size)"
          }
          aria-label="Toggle subnet input mode"
          className="flex size-8 items-center justify-center rounded-md text-ink-3 transition-colors duration-snap hover:bg-sunken hover:text-ink"
        >
          <SlidersHorizontal aria-hidden className="size-4" />
        </button>

        {mode === "simple" ? (
          <Select
            value={String(prefix)}
            onValueChange={(v) => setPrefixValue(v)}
          >
            <SelectTrigger
              data-testid={`${t}-size`}
              aria-label="Subnet size"
              className="w-48"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SIZE_OPTIONS.map((o) => (
                <SelectItem key={o.prefix} value={String(o.prefix)}>
                  Up to {o.hosts} devices (/{o.prefix})
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        ) : (
          <Select
            value={String(prefix)}
            onValueChange={(v) => setPrefixValue(v)}
          >
            <SelectTrigger
              data-testid={`${t}-prefix`}
              aria-label="Prefix length"
              className="w-28"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {ADVANCED_PREFIXES.map((p) => (
                <SelectItem key={p} value={String(p)}>
                  /{p}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
      </div>

      {resolved && !isPrivate ? (
        <Text size="xs" className="text-danger" data-testid={`${t}-cidr`}>
          Must be a private range (10.x, 172.16–31.x, or 192.168.x).
        </Text>
      ) : (
        <Text size="xs" className="text-ink-3" data-testid={`${t}-cidr`}>
          {resolved
            ? `Resolves to ${resolved} — ${usableHosts(prefix)} usable hosts`
            : "Enter a base address to build the subnet."}
        </Text>
      )}
    </div>
  );
}
