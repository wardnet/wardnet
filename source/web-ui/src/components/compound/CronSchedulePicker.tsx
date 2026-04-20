import { useState } from "react";
import { CalendarClock, ChevronDown } from "lucide-react";
import { cronToHuman } from "@/lib/cron";
import { Button } from "@/components/core/ui/button";
import { Label } from "@/components/core/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/core/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/core/ui/select";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type Frequency = "hourly" | "every-n-hours" | "daily" | "weekly" | "monthly";

interface ScheduleState {
  frequency: Frequency;
  intervalHours: string; // "2" | "3" | "4" | "6" | "8" | "12"
  hour: string; // "0".."23"
  /** Days of the week (Sun=0, Sat=6). Multiple days allowed for weekly. */
  daysOfWeek: number[];
  dayOfMonth: string; // "1".."28"
}

// ---------------------------------------------------------------------------
// Parse a cron string into editable state
// ---------------------------------------------------------------------------

function parseDayList(field: string): number[] | null {
  const parts = field.split(",").map((p) => p.trim());
  const result: number[] = [];
  for (const p of parts) {
    if (!/^\d+$/.test(p)) return null;
    const n = parseInt(p, 10);
    if (n < 0 || n > 6) return null;
    result.push(n);
  }
  return result.length > 0 ? Array.from(new Set(result)).sort((a, b) => a - b) : null;
}

function parseCron(expr: string): ScheduleState {
  const base: ScheduleState = {
    frequency: "daily",
    intervalHours: "6",
    hour: "3",
    daysOfWeek: [1], // default weekly = Monday
    dayOfMonth: "1",
  };

  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return base;

  const [min, h, dom, , dow] = parts;

  if (min === "0" && h === "*" && dom === "*" && dow === "*") {
    return { ...base, frequency: "hourly" };
  }
  const everyN = h.match(/^\*\/(\d+)$/);
  if (min === "0" && everyN && dom === "*" && dow === "*") {
    return { ...base, frequency: "every-n-hours", intervalHours: everyN[1] };
  }
  if (min === "0" && /^\d+$/.test(h) && dom === "*" && dow === "*") {
    return { ...base, frequency: "daily", hour: h };
  }
  if (min === "0" && /^\d+$/.test(h) && dom === "*") {
    const days = parseDayList(dow);
    if (days) return { ...base, frequency: "weekly", hour: h, daysOfWeek: days };
  }
  if (min === "0" && /^\d+$/.test(h) && /^\d+$/.test(dom) && dow === "*") {
    return { ...base, frequency: "monthly", hour: h, dayOfMonth: dom };
  }

  return base;
}

// ---------------------------------------------------------------------------
// Build cron from state
// ---------------------------------------------------------------------------

function buildCron(s: ScheduleState): string {
  switch (s.frequency) {
    case "hourly":
      return "0 * * * *";
    case "every-n-hours":
      return `0 */${s.intervalHours} * * *`;
    case "daily":
      return `0 ${s.hour} * * *`;
    case "weekly": {
      // Fall back to Monday if somehow no day is selected (button states
      // enforce this, but the cron must still be valid).
      const days = s.daysOfWeek.length > 0 ? s.daysOfWeek : [1];
      return `0 ${s.hour} * * ${days.join(",")}`;
    }
    case "monthly":
      return `0 ${s.hour} ${s.dayOfMonth} * *`;
  }
}

// ---------------------------------------------------------------------------
// Picker component
// ---------------------------------------------------------------------------

const DOW_LABELS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DOW_SHORT = ["S", "M", "T", "W", "T", "F", "S"];
const DOM_ORDINALS: Record<string, string> = Object.fromEntries(
  Array.from({ length: 28 }, (_, i) => {
    const n = i + 1;
    const sfx = n === 1 ? "st" : n === 2 ? "nd" : n === 3 ? "rd" : "th";
    return [String(n), `${n}${sfx}`];
  }),
);

function hourLabel(h: string): string {
  const n = parseInt(h, 10);
  if (n === 0) return "12:00 AM";
  if (n < 12) return `${n}:00 AM`;
  if (n === 12) return "12:00 PM";
  return `${n - 12}:00 PM`;
}

interface CronSchedulePickerProps {
  value: string;
  onChange: (value: string) => void;
  label?: string;
}

/** Popover-based schedule builder. Shows a human-readable label as the trigger. */
export function CronSchedulePicker({ value, onChange, label }: CronSchedulePickerProps) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<ScheduleState>(() => parseCron(value));

  function update(patch: Partial<ScheduleState>) {
    const next = { ...state, ...patch };
    setState(next);
    onChange(buildCron(next));
  }

  const hours = Array.from({ length: 24 }, (_, i) => String(i));
  const nHoursOptions = ["2", "3", "4", "6", "8", "12"];
  const domOptions = Array.from({ length: 28 }, (_, i) => String(i + 1));

  return (
    <div className="flex flex-col gap-1.5">
      {label && <Label>{label}</Label>}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button variant="outline" className="w-full justify-between font-normal">
            <span className="flex min-w-0 items-center gap-2">
              <CalendarClock className="size-4 shrink-0 text-muted-foreground" />
              <span className="truncate">{cronToHuman(value)}</span>
            </span>
            <ChevronDown className="size-4 shrink-0 text-muted-foreground" />
          </Button>
        </PopoverTrigger>

        <PopoverContent className="w-80" align="start">
          <div className="flex flex-col gap-4 p-1">
            {/* Frequency */}
            <div className="flex flex-col gap-1.5">
              <p className="text-xs font-medium text-muted-foreground">Repeat</p>
              <Select
                value={state.frequency}
                onValueChange={(v) => update({ frequency: v as Frequency })}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="hourly">Every hour</SelectItem>
                  <SelectItem value="every-n-hours">Every N hours</SelectItem>
                  <SelectItem value="daily">Daily</SelectItem>
                  <SelectItem value="weekly">Weekly</SelectItem>
                  <SelectItem value="monthly">Monthly</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {/* Interval — every N hours */}
            {state.frequency === "every-n-hours" && (
              <div className="flex flex-col gap-1.5">
                <p className="text-xs font-medium text-muted-foreground">Interval</p>
                <Select
                  value={state.intervalHours}
                  onValueChange={(v) => update({ intervalHours: v })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {nHoursOptions.map((n) => (
                      <SelectItem key={n} value={n}>
                        Every {n} hours
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {/* Days of week — weekly (multi-select toggle row) */}
            {state.frequency === "weekly" && (
              <div className="flex flex-col gap-1.5">
                <p className="text-xs font-medium text-muted-foreground">Days</p>
                <div className="flex gap-1">
                  {DOW_SHORT.map((label, i) => {
                    const active = state.daysOfWeek.includes(i);
                    return (
                      <Button
                        key={i}
                        type="button"
                        variant={active ? "default" : "outline"}
                        size="sm"
                        className="flex-1 px-0"
                        aria-label={DOW_LABELS[i]}
                        aria-pressed={active}
                        onClick={() => {
                          const next = active
                            ? state.daysOfWeek.filter((d) => d !== i)
                            : [...state.daysOfWeek, i].sort((a, b) => a - b);
                          // Never allow zero selected days — cron would be invalid.
                          if (next.length === 0) return;
                          update({ daysOfWeek: next });
                        }}
                      >
                        {label}
                      </Button>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Day of month — monthly */}
            {state.frequency === "monthly" && (
              <div className="flex flex-col gap-1.5">
                <p className="text-xs font-medium text-muted-foreground">Day of month</p>
                <Select value={state.dayOfMonth} onValueChange={(v) => update({ dayOfMonth: v })}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {domOptions.map((d) => (
                      <SelectItem key={d} value={d}>
                        {DOM_ORDINALS[d]}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {/* Time — daily / weekly / monthly */}
            {(state.frequency === "daily" ||
              state.frequency === "weekly" ||
              state.frequency === "monthly") && (
              <div className="flex flex-col gap-1.5">
                <p className="text-xs font-medium text-muted-foreground">At</p>
                <Select value={state.hour} onValueChange={(v) => update({ hour: v })}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {hours.map((h) => (
                      <SelectItem key={h} value={h}>
                        {hourLabel(h)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            )}

            {/* Human-readable summary */}
            <div className="rounded-md bg-muted/50 px-3 py-2">
              <p className="text-xs text-muted-foreground">{cronToHuman(buildCron(state))}</p>
            </div>

            <Button size="sm" className="w-full" onClick={() => setOpen(false)}>
              Done
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  );
}
