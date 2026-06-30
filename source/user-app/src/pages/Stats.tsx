import { useState } from "react";
import {
  ChartColumnIcon,
  MessageSquarePlusIcon,
  ShieldOffIcon,
  WifiOffIcon,
} from "lucide-react";
import { Link } from "react-router";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Pill,
  Sparkline,
  StatTile,
  Text,
  useMyDevice,
} from "@wardnet/web";

import { todayLocal } from "@/lib/dnsDb";
import { useDnsStats } from "@/hooks/useDnsStats";
import {
  RequestRuleModal,
  type RequestTarget,
} from "@/features/RequestRuleModal";
import type { DomainCount } from "@/lib/dnsStats";
import type { DnsEventItem } from "@/lib/dnsDb";

function weekdayLabel(date: string, today: string): string {
  if (date === today) return "Today";
  // date is YYYY-MM-DD (local); build a local Date for the weekday.
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, {
    weekday: "short",
  });
}

function relativeTime(iso: string): string {
  const sec = Math.round((Date.now() - new Date(iso).getTime()) / 1000);
  if (sec < 60) return "just now";
  const min = Math.round(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.round(hr / 24)}d ago`;
}

function pct(part: number, whole: number): number {
  return whole > 0 ? Math.round((part / whole) * 100) : 0;
}

function DomainList({
  title,
  rows,
  emptyText,
  variant,
  onRequest,
}: {
  title: string;
  rows: DomainCount[];
  emptyText: string;
  variant: "warn" | "info";
  /** Called when the user taps the "ask admin" action on a domain row. */
  onRequest: (domain: string) => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <Text as="p" size="sm" className="text-ink-3">
            {emptyText}
          </Text>
        ) : (
          <ul className="flex flex-col gap-2">
            {rows.map((r) => (
              <li
                key={r.domain}
                className="flex items-center justify-between gap-3"
              >
                <Text as="span" size="sm" className="truncate font-mono text-ink">
                  {r.domain}
                </Text>
                <span className="flex shrink-0 items-center gap-2">
                  <Pill variant={variant}>{r.count.toLocaleString()}</Pill>
                  <button
                    type="button"
                    onClick={() => onRequest(r.domain)}
                    aria-label={`Ask admin about ${r.domain}`}
                    className="text-ink-3 active:text-accent"
                  >
                    <MessageSquarePlusIcon className="size-4" />
                  </button>
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function ActivityFeed({
  events,
  onRequest,
}: {
  events: DnsEventItem[];
  /** Ask-admin action for a single event; default kind derives from status. */
  onRequest: (domain: string, kind: "block" | "allow") => void;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent activity</CardTitle>
      </CardHeader>
      <CardContent>
        {events.length === 0 ? (
          <Text as="p" size="sm" className="text-ink-3">
            No recent activity.
          </Text>
        ) : (
          <ul className="flex flex-col gap-2">
            {events.map((e) => {
              const blocked = e.status === "blocked";
              return (
                <li
                  key={e.id}
                  className="flex items-center justify-between gap-3"
                >
                  <Text as="span" size="sm" className="truncate font-mono text-ink">
                    {e.domain}
                  </Text>
                  <span className="flex shrink-0 items-center gap-2">
                    {blocked ? (
                      <Pill variant="warn">Blocked</Pill>
                    ) : (
                      <Pill variant="ok">Allowed</Pill>
                    )}
                    <Text as="span" size="xs" className="text-ink-3">
                      {relativeTime(e.captured_at)}
                    </Text>
                    <button
                      type="button"
                      onClick={() =>
                        onRequest(e.domain, blocked ? "allow" : "block")
                      }
                      aria-label={`Ask admin about ${e.domain}`}
                      className="text-ink-3 active:text-accent"
                    >
                      <MessageSquarePlusIcon className="size-4" />
                    </button>
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * Stats tab — rich, history-aware personal DNS stats read entirely from the
 * device-local IndexedDB store (no daemon query). Shows a selectable day, a
 * 7-day trend, headline counters, top blocked/queried domains, and a recent
 * activity feed.
 */
export default function Stats() {
  const today = todayLocal();
  const [date, setDate] = useState(today);
  const [requestTarget, setRequestTarget] = useState<RequestTarget | null>(null);
  const { data: me, isLoading: meLoading } = useMyDevice();
  const stats = useDnsStats(date);

  const device = me?.device;

  if (meLoading) {
    return (
      <Text as="p" size="sm" className="p-5 text-ink-3">
        Loading…
      </Text>
    );
  }

  if (!device) {
    return (
      <div
        data-testid="stats-no-device"
        className="flex flex-col items-center gap-4 px-5 py-16 text-center"
      >
        <WifiOffIcon className="size-12 text-ink-3/50" />
        <Text as="h1" size="lg" weight="semibold" className="text-ink">
          Device not detected
        </Text>
        <Text as="p" size="sm" className="max-w-md text-ink-3">
          Your device has not been detected on the network yet.
        </Text>
      </div>
    );
  }

  if (!device.dns_capture_enabled) {
    return (
      <div
        data-testid="stats-capture-off"
        className="flex flex-col items-center gap-4 px-5 py-16 text-center"
      >
        <ShieldOffIcon className="size-12 text-ink-3/50" strokeWidth={1.5} />
        <Text as="h1" size="lg" weight="semibold" className="text-ink">
          DNS capture is off
        </Text>
        <Text as="p" size="sm" className="max-w-md text-ink-3">
          Turn on DNS capture to start collecting stats for your device. Your
          data stays on this device.
        </Text>
        <Link
          to="/settings"
          data-testid="stats-settings-link"
          className="rounded-full bg-accent px-4 py-1.5 text-sm font-medium text-accent-ink"
        >
          Go to Settings
        </Link>
      </div>
    );
  }

  if (!stats.hasData) {
    return (
      <div
        data-testid="stats-waiting"
        className="flex flex-col items-center gap-4 px-5 py-16 text-center"
      >
        <ChartColumnIcon className="size-12 text-ink-3/50" strokeWidth={1.5} />
        <Text as="h1" size="lg" weight="semibold" className="text-ink">
          Waiting for DNS activity…
        </Text>
        <Text as="p" size="sm" className="max-w-md text-ink-3">
          DNS capture is on. As soon as your device makes DNS queries they will
          show up here.
        </Text>
      </div>
    );
  }

  const { headline, trend, topBlocked, topQueried, recent } = stats;
  const blockedPct = pct(headline.blocked, headline.total);

  return (
    <div data-testid="stats-content" className="flex flex-col gap-6 p-5">
      <Text as="h1" size="lg" weight="semibold" className="text-ink">
        DNS stats
      </Text>

      {/* 7-day trend with a clickable day scope strip. */}
      <Card>
        <CardHeader>
          <CardTitle>Last {trend.length} days</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="h-12 w-full">
            <Sparkline
              values={trend.map((t) => t.blocked)}
              color="var(--warn)"
              className="h-full w-full"
            />
          </div>
          <div className="flex justify-between gap-1">
            {trend.map((t) => {
              const selected = t.date === date;
              return (
                <button
                  key={t.date}
                  onClick={() => setDate(t.date)}
                  className={`flex flex-1 flex-col items-center rounded-lg px-1 py-1.5 text-[11px] transition-colors ${
                    selected
                      ? "bg-accent/15 font-semibold text-accent"
                      : "text-ink-3 active:bg-sunken"
                  }`}
                >
                  <span>{weekdayLabel(t.date, today)}</span>
                  <span className="mt-0.5 text-ink">{t.blocked}</span>
                </button>
              );
            })}
          </div>
        </CardContent>
      </Card>

      {/* Headline counters for the selected day. */}
      <div className="grid grid-cols-3 gap-3">
        <StatTile label="Queries" value={headline.total.toLocaleString()} />
        <StatTile
          label="Blocked"
          value={headline.blocked.toLocaleString()}
          sub={`${blockedPct}%`}
        />
        <StatTile label="Allowed" value={headline.allowed.toLocaleString()} />
      </div>

      {/* Blocked domains default to an "allow" (unblock) request. */}
      <DomainList
        title="Top blocked"
        rows={topBlocked}
        emptyText="Nothing blocked on this day."
        variant="warn"
        onRequest={(domain) => setRequestTarget({ domain, kind: "allow" })}
      />
      {/* Queried domains default to a "block" request. */}
      <DomainList
        title="Most queried"
        rows={topQueried}
        emptyText="No queries on this day."
        variant="info"
        onRequest={(domain) => setRequestTarget({ domain, kind: "block" })}
      />

      {/* Recent activity is global (latest events), not day-scoped. */}
      <ActivityFeed
        events={recent}
        onRequest={(domain, kind) => setRequestTarget({ domain, kind })}
      />

      <RequestRuleModal
        target={requestTarget}
        onClose={() => setRequestTarget(null)}
      />
    </div>
  );
}
