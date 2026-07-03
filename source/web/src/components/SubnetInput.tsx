import { useState } from "react";
import { Input, Text } from "@wardnet/ui";
import {
  buildCidr,
  octetsValid,
  parseCidr,
  prefixForHosts,
  usableHosts,
} from "../lib/cidr";

interface SubnetInputProps {
  /** Current value as a normalized network CIDR (e.g. "10.44.0.0/26"), or "". */
  value: string;
  /** Called with the resolved CIDR, or "" while the inputs are incomplete. */
  onChange: (cidr: string) => void;
  id?: string;
  testId?: string;
}

type Mode = "simple" | "advanced";
type Octets = [string, string, string, string];

const EMPTY: Octets = ["", "", "", ""];

function toOctets(value: string): { octets: Octets; prefix: number } {
  const parsed = parseCidr(value);
  if (!parsed) return { octets: EMPTY, prefix: 24 };
  return {
    octets: parsed.octets.map(String) as Octets,
    prefix: parsed.prefix,
  };
}

function numericOctets(
  octets: Octets,
): [number, number, number, number] | null {
  const nums = octets.map((o) => (o === "" ? NaN : Number(o)));
  return octetsValid(nums) ? (nums as [number, number, number, number]) : null;
}

/**
 * Friendly IPv4 subnet picker. Two modes over the same value:
 *  - **Simple**: a base address + a "number of devices" that picks the prefix.
 *  - **Advanced**: a base address + an explicit prefix length.
 * Both resolve to a normalized network CIDR, shown with its usable-host count.
 * Emits `""` until the address is complete. Reusable anywhere a CIDR is needed.
 */
export function SubnetInput({ value, onChange, id, testId }: SubnetInputProps) {
  const initial = toOctets(value);
  const [mode, setMode] = useState<Mode>("simple");
  const [octets, setOctets] = useState<Octets>(initial.octets);
  const [prefix, setPrefix] = useState<number>(initial.prefix);
  // The device count is what the user types in simple mode; the prefix is
  // derived from it. Kept separate so typing isn't clobbered by the derived
  // usable-host figure.
  const [devices, setDevices] = useState<string>(
    String(usableHosts(initial.prefix)),
  );

  const nums = numericOctets(octets);
  const resolved = nums ? buildCidr(nums, prefix) : "";
  const t = testId ?? "subnet";

  function emit(nextOctets: Octets, nextPrefix: number) {
    const n = numericOctets(nextOctets);
    onChange(n ? buildCidr(n, nextPrefix) : "");
  }

  function setOctet(i: number, raw: string) {
    const clean = raw.replace(/\D/g, "").slice(0, 3);
    const next = [...octets] as Octets;
    next[i] = clean;
    setOctets(next);
    emit(next, prefix);
  }

  function setDeviceCount(raw: string) {
    const clean = raw.replace(/\D/g, "");
    setDevices(clean);
    const nextPrefix = prefixForHosts(Number(clean));
    setPrefix(nextPrefix);
    emit(octets, nextPrefix);
  }

  function setPrefixValue(raw: string) {
    const n = Number(raw.replace(/\D/g, ""));
    const nextPrefix = Number.isFinite(n)
      ? Math.min(32, Math.max(0, n))
      : prefix;
    setPrefix(nextPrefix);
    setDevices(String(usableHosts(nextPrefix)));
    emit(octets, nextPrefix);
  }

  const modeBtn = (m: Mode, label: string) => (
    <button
      type="button"
      data-testid={`${t}-mode-${m}`}
      onClick={() => setMode(m)}
      className={[
        "rounded-md px-2.5 py-1 text-xs font-medium transition-colors duration-snap",
        mode === m ? "bg-accent text-accent-ink" : "bg-sunken text-ink-3",
      ].join(" ")}
    >
      {label}
    </button>
  );

  return (
    <div className="flex flex-col gap-2" id={id}>
      <div className="flex gap-1.5">
        {modeBtn("simple", "Simple")}
        {modeBtn("advanced", "Advanced")}
      </div>

      <div className="flex flex-wrap items-end gap-3">
        <div className="flex flex-col gap-1">
          <Text size="2xs" className="uppercase tracking-wide text-ink-3">
            Base address
          </Text>
          <div className="flex items-center gap-1">
            {octets.map((o, i) => (
              <span key={i} className="flex items-center gap-1">
                <Input
                  aria-label={`Base address octet ${i + 1}`}
                  data-testid={`${t}-octet-${i}`}
                  inputMode="numeric"
                  className="w-12 text-center"
                  value={o}
                  onChange={(e) => setOctet(i, e.target.value)}
                  placeholder="0"
                />
                {i < 3 && <span className="text-ink-3">.</span>}
              </span>
            ))}
          </div>
        </div>

        {mode === "simple" ? (
          <div className="flex flex-col gap-1">
            <Text size="2xs" className="uppercase tracking-wide text-ink-3">
              Number of devices
            </Text>
            <Input
              aria-label="Number of devices"
              data-testid={`${t}-devices`}
              inputMode="numeric"
              className="w-28"
              value={devices}
              onChange={(e) => setDeviceCount(e.target.value)}
            />
          </div>
        ) : (
          <div className="flex flex-col gap-1">
            <Text size="2xs" className="uppercase tracking-wide text-ink-3">
              Prefix (/bits)
            </Text>
            <Input
              aria-label="Prefix length"
              data-testid={`${t}-prefix`}
              inputMode="numeric"
              className="w-24"
              value={String(prefix)}
              onChange={(e) => setPrefixValue(e.target.value)}
            />
          </div>
        )}
      </div>

      <Text
        size="xs"
        className="text-ink-3"
        data-testid={`${t}-cidr`}
        title={
          resolved ? `${usableHosts(prefix)} usable host addresses` : undefined
        }
      >
        {resolved ? (
          <>
            = <span className="font-mono text-ink">{resolved}</span>{" "}
            <span aria-hidden>ⓘ</span> {usableHosts(prefix)} usable hosts
          </>
        ) : (
          "Enter a base address to build the subnet."
        )}
      </Text>
    </div>
  );
}
