import { useRef, useCallback, type KeyboardEvent, type ClipboardEvent } from "react";
import { cn } from "@/lib/utils";

interface Ipv4InputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  readOnly?: boolean;
  className?: string;
  id?: string;
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
  className,
  id,
}: Ipv4InputProps) {
  const refs = [
    useRef<HTMLInputElement>(null),
    useRef<HTMLInputElement>(null),
    useRef<HTMLInputElement>(null),
    useRef<HTMLInputElement>(null),
  ];

  const octets = parseOctets(value);
  const placeholders = parseOctets(placeholder);

  const updateOctet = useCallback(
    (index: number, raw: string) => {
      // Strip non-numeric characters.
      const digits = raw.replace(/\D/g, "");
      // Clamp to 0-255.
      let num = digits === "" ? "" : String(Math.min(255, parseInt(digits, 10)));
      if (num === "NaN") num = "";

      const next: [string, string, string, string] = [...octets];
      next[index] = num;
      onChange(joinOctets(next));

      // Auto-tab to next segment when 3 digits entered or value is complete.
      if (num.length === 3 || (num.length > 0 && parseInt(num, 10) > 25)) {
        if (index < 3) {
          refs[index + 1].current?.focus();
          refs[index + 1].current?.select();
        }
      }
    },
    [octets, onChange, refs],
  );

  const handleKeyDown = useCallback(
    (index: number, e: KeyboardEvent<HTMLInputElement>) => {
      const input = e.currentTarget;

      if (e.key === "." || e.key === "Tab") {
        if (e.key === ".") e.preventDefault();
        if (index < 3 && e.key === ".") {
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

      if (e.key === "ArrowRight" && input.selectionStart === input.value.length && index < 3) {
        e.preventDefault();
        refs[index + 1].current?.focus();
      }
    },
    [refs],
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
      className={cn(
        "flex h-9 items-center gap-0 rounded-md border border-input bg-transparent px-1 text-sm font-mono shadow-xs transition-colors focus-within:ring-1 focus-within:ring-ring",
        disabled && "cursor-not-allowed opacity-50",
        readOnly && "bg-muted",
        className,
      )}
    >
      {octets.map((octet, i) => (
        <div key={i} className="flex items-center">
          <input
            ref={refs[i]}
            id={i === 0 ? id : undefined}
            type="text"
            inputMode="numeric"
            value={octet}
            placeholder={placeholders[i]}
            disabled={disabled}
            readOnly={readOnly}
            onChange={(e) => updateOctet(i, e.target.value)}
            onKeyDown={(e) => handleKeyDown(i, e)}
            onPaste={i === 0 ? handlePaste : undefined}
            onFocus={(e) => e.target.select()}
            className="w-10 bg-transparent text-center outline-none placeholder:text-muted-foreground/40"
            maxLength={3}
          />
          {i < 3 && <span className="text-muted-foreground/40">.</span>}
        </div>
      ))}
    </div>
  );
}
