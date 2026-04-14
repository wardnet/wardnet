import { useRef, useCallback, type KeyboardEvent, type ClipboardEvent } from "react";
import { cn } from "@/lib/utils";

interface MacInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  readOnly?: boolean;
  className?: string;
  id?: string;
}

function parseSegments(value: string): [string, string, string, string, string, string] {
  const clean = value.replace(/[^a-fA-F0-9]/g, "");
  const parts: string[] = [];
  for (let i = 0; i < 6; i++) {
    parts.push(clean.slice(i * 2, i * 2 + 2));
  }
  return [
    parts[0] ?? "",
    parts[1] ?? "",
    parts[2] ?? "",
    parts[3] ?? "",
    parts[4] ?? "",
    parts[5] ?? "",
  ];
}

function joinSegments(segs: string[]): string {
  if (segs.every((s) => s === "")) return "";
  return segs.map((s) => s.toUpperCase()).join(":");
}

/** MAC address input with 6 hex segments and auto-tabbing. */
export function MacInput({
  value,
  onChange,
  placeholder = "AA:BB:CC:DD:EE:FF",
  disabled,
  readOnly,
  className,
  id,
}: MacInputProps) {
  // Six individual useRef calls (hook count must be constant and not called
  // inside a callback — `Array.from(() => useRef(...))` violates both).
  const ref0 = useRef<HTMLInputElement>(null);
  const ref1 = useRef<HTMLInputElement>(null);
  const ref2 = useRef<HTMLInputElement>(null);
  const ref3 = useRef<HTMLInputElement>(null);
  const ref4 = useRef<HTMLInputElement>(null);
  const ref5 = useRef<HTMLInputElement>(null);
  const refs = [ref0, ref1, ref2, ref3, ref4, ref5];
  const segments = parseSegments(value);
  const placeholderSegs = placeholder.split(":").map((s) => s.slice(0, 2));

  const updateSegment = useCallback(
    (index: number, raw: string) => {
      const hex = raw.replace(/[^a-fA-F0-9]/g, "").slice(0, 2);
      const next = [...segments];
      next[index] = hex;
      onChange(joinSegments(next));

      if (hex.length === 2 && index < 5) {
        refs[index + 1].current?.focus();
        refs[index + 1].current?.select();
      }
    },
    [segments, onChange, refs],
  );

  const handleKeyDown = useCallback(
    (index: number, e: KeyboardEvent<HTMLInputElement>) => {
      const input = e.currentTarget;

      if (e.key === ":" || e.key === "-" || e.key === "Tab") {
        if (e.key !== "Tab") e.preventDefault();
        if (index < 5 && e.key !== "Tab") {
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

      if (e.key === "ArrowRight" && input.selectionStart === input.value.length && index < 5) {
        e.preventDefault();
        refs[index + 1].current?.focus();
      }
    },
    [refs],
  );

  const handlePaste = useCallback(
    (e: ClipboardEvent<HTMLInputElement>) => {
      const pasted = e.clipboardData.getData("text").trim();
      // Accept formats: AA:BB:CC:DD:EE:FF, AA-BB-CC-DD-EE-FF, AABBCCDDEEFF
      const clean = pasted.replace(/[^a-fA-F0-9]/g, "");
      if (clean.length === 12) {
        e.preventDefault();
        const segs: string[] = [];
        for (let i = 0; i < 6; i++) {
          segs.push(clean.slice(i * 2, i * 2 + 2));
        }
        onChange(joinSegments(segs));
        refs[5].current?.focus();
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
      {segments.map((seg, i) => (
        <div key={i} className="flex items-center">
          <input
            ref={refs[i]}
            id={i === 0 ? id : undefined}
            type="text"
            value={seg.toUpperCase()}
            placeholder={placeholderSegs[i] ?? "00"}
            disabled={disabled}
            readOnly={readOnly}
            onChange={(e) => updateSegment(i, e.target.value)}
            onKeyDown={(e) => handleKeyDown(i, e)}
            onPaste={i === 0 ? handlePaste : undefined}
            onFocus={(e) => e.target.select()}
            className="w-7 bg-transparent text-center uppercase outline-none placeholder:text-muted-foreground/40"
            maxLength={2}
          />
          {i < 5 && <span className="text-muted-foreground/40">:</span>}
        </div>
      ))}
    </div>
  );
}
