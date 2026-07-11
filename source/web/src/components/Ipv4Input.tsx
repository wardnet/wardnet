import {
  useRef,
  useCallback,
  useMemo,
  type KeyboardEvent,
  type ClipboardEvent,
} from "react";
import { cn } from "../lib/utils";

interface Ipv4InputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  readOnly?: boolean;
  /** Lock the last N octets to `0` (read-only) — used by the subnet picker to
   *  keep the host portion out of the operator's hands. */
  readOnlyOctets?: number;
  className?: string;
  id?: string;
  /** Forwarded onto the field wrapper so e2e specs can locate it and
   *  fill each octet input individually (`getByTestId(id).locator("input")`).
   *  Per-segment filling avoids the auto-tab-on-`.` behaviour, which
   *  can skip an octet that another octet's >25 auto-advance already
   *  jumped past. */
  "data-testid"?: string;
}

function parseOctets(value: string): [string, string, string, string] {
  const parts = value.split(".");
  return [parts[0] ?? "", parts[1] ?? "", parts[2] ?? "", parts[3] ?? ""];
}

function joinOctets(octets: [string, string, string, string]): string {
  if (octets.every((o) => o === "")) return "";
  return octets.join(".");
}

/** IPv4 address input with 4 octet segments and auto-tabbing. */
export function Ipv4Input({
  value,
  onChange,
  placeholder = "0.0.0.0",
  disabled,
  readOnly,
  readOnlyOctets = 0,
  className,
  id,
  "data-testid": dataTestId,
}: Ipv4InputProps) {
  const lockedFrom = 4 - Math.min(4, Math.max(0, readOnlyOctets));
  const ref0 = useRef<HTMLInputElement>(null);
  const ref1 = useRef<HTMLInputElement>(null);
  const ref2 = useRef<HTMLInputElement>(null);
  const ref3 = useRef<HTMLInputElement>(null);
  const refs = useMemo(
    () => [ref0, ref1, ref2, ref3],
    [ref0, ref1, ref2, ref3],
  );

  const octets = parseOctets(value);
  const placeholders = parseOctets(placeholder);

  const updateOctet = useCallback(
    (index: number, raw: string) => {
      // Strip non-numeric characters.
      const digits = raw.replace(/\D/g, "");
      // Clamp to 0-255.
      let num =
        digits === "" ? "" : String(Math.min(255, parseInt(digits, 10)));
      if (num === "NaN") num = "";

      const next: [string, string, string, string] = [...octets];
      // eslint-disable-next-line security/detect-object-injection -- index is the 0-3 octet position from a 4-element map, writing into a fixed 4-tuple
      next[index] = num;
      onChange(joinOctets(next));

      // Auto-tab to next segment when 3 digits entered or value is complete —
      // but never into a locked (read-only) octet.
      if (num.length === 3 || (num.length > 0 && parseInt(num, 10) > 25)) {
        if (index + 1 < lockedFrom) {
          refs[index + 1].current?.focus();
          refs[index + 1].current?.select();
        }
      }
    },
    [octets, onChange, refs, lockedFrom],
  );

  const handleKeyDown = useCallback(
    (index: number, e: KeyboardEvent<HTMLInputElement>) => {
      const input = e.currentTarget;

      if (e.key === "." || e.key === "Tab") {
        if (e.key === ".") e.preventDefault();
        if (index + 1 < lockedFrom && e.key === ".") {
          refs[index + 1].current?.focus();
          refs[index + 1].current?.select();
        }
        return;
      }

      if (e.key === "Backspace" && input.value === "" && index > 0) {
        e.preventDefault();
        refs[index - 1].current?.focus();
        refs[index - 1].current?.select();
      }

      if (e.key === "ArrowLeft" && input.selectionStart === 0 && index > 0) {
        e.preventDefault();
        refs[index - 1].current?.focus();
      }

      if (
        e.key === "ArrowRight" &&
        input.selectionStart === input.value.length &&
        index < 3
      ) {
        e.preventDefault();
        refs[index + 1].current?.focus();
      }
    },
    [refs, lockedFrom],
  );

  const handlePaste = useCallback(
    (e: ClipboardEvent<HTMLInputElement>) => {
      const pasted = e.clipboardData.getData("text").trim();
      // If it looks like a full IP, parse and fill all segments.
      if (/^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(pasted)) {
        e.preventDefault();
        const parts = pasted.split(".");
        const clamped: [string, string, string, string] = [
          String(Math.min(255, parseInt(parts[0], 10))),
          String(Math.min(255, parseInt(parts[1], 10))),
          String(Math.min(255, parseInt(parts[2], 10))),
          String(Math.min(255, parseInt(parts[3], 10))),
        ];
        onChange(joinOctets(clamped));
        refs[3].current?.focus();
      }
    },
    [onChange, refs],
  );

  return (
    <div
      data-segmented
      data-testid={dataTestId}
      className={cn(
        "input",
        disabled && "cursor-not-allowed opacity-60",
        readOnly && "!bg-sunken",
        className,
      )}
    >
      {octets.map((octet, i) => {
        const locked = i >= lockedFrom;
        return (
          <div key={i} className="flex items-center">
            <input
              // eslint-disable-next-line security/detect-object-injection -- i is the 0-3 map index over four local octet refs
              ref={refs[i]}
              id={i === 0 ? id : undefined}
              type="text"
              inputMode="numeric"
              value={locked ? "0" : octet}
              // eslint-disable-next-line security/detect-object-injection -- i is the 0-3 map index over the locally parsed placeholder tuple
              placeholder={placeholders[i]}
              disabled={disabled}
              readOnly={readOnly || locked}
              tabIndex={locked ? -1 : undefined}
              onChange={(e) => !locked && updateOctet(i, e.target.value)}
              onKeyDown={(e) => handleKeyDown(i, e)}
              onPaste={i === 0 ? handlePaste : undefined}
              onFocus={(e) => e.target.select()}
              className={cn("!w-10", locked && "text-ink-3/50")}
              maxLength={3}
            />
            {i < 3 && <span className="text-ink-3/40">.</span>}
          </div>
        );
      })}
    </div>
  );
}
