import { GlobeIcon, ShieldOffIcon } from "lucide-react";
import { Link } from "react-router";
import { Card, Text } from "@wardnet/web";
import { Sparkline } from "@wardnet/web";
import type { DashboardDnsStats } from "@wardnet/web";

interface Props {
  data: DashboardDnsStats | undefined;
  isLoading: boolean;
}

export function DnsQueriesCard({ data, isLoading }: Props) {
  return (
    <Link to="/dns" className="block" data-testid="dashboard-dns-queries-card">
      <Card className="card--flush">
        <div className="flex items-center gap-3 px-2 py-3">
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style={{
              background: "color-mix(in srgb, var(--color-ink-3) 12%, transparent)",
              color: "var(--color-ink-3)",
            }}
          >
            <GlobeIcon size={20} strokeWidth={1.8} />
          </div>

          <div className="min-w-0 shrink-0">
            <Text as="div" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
              DNS Queries · 24h
            </Text>
            <Text as="div" size="2xl" weight="bold" className="mt-0.5 text-ink tabular-nums">
              {isLoading && !data ? "…" : (data?.total.toLocaleString() ?? "0")}
            </Text>
          </div>

          <div className="h-12 flex-1 opacity-70">
            {data && data.totalSeries.length > 0 && (
              <Sparkline values={data.totalSeries} color="var(--color-ink-3)" area={false} />
            )}
          </div>
        </div>
      </Card>
    </Link>
  );
}

export function BlockedCard({ data, isLoading }: Props) {
  const sub =
    data != null
      ? `${data.blocked.toLocaleString()} of ${data.total.toLocaleString()}`
      : undefined;

  return (
    <Link to="/dns" className="block" data-testid="dashboard-blocked-card">
      <Card className="card--flush">
        <div className="flex items-center gap-3 px-2 py-3">
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style={{ background: "var(--color-warn-soft)", color: "var(--color-warn)" }}
          >
            <ShieldOffIcon size={20} strokeWidth={1.8} />
          </div>

          <div className="min-w-0 shrink-0">
            <Text as="div" size="2xs" weight="semibold" className="uppercase tracking-wider text-ink-3">
              Blocked · 24h
            </Text>
            <div className="mt-0.5 flex items-baseline gap-0.5">
              <Text as="span" size="2xl" weight="bold" className="text-ink tabular-nums">
                {isLoading && !data ? "…" : `${(data?.blockedPercent ?? 0).toFixed(1)}`}
              </Text>
              <Text as="span" size="sm" className="text-ink-3">%</Text>
            </div>
            {sub && <Text as="div" size="xs" className="mt-0.5 text-ink-3">{sub}</Text>}
          </div>

          <div className="h-12 flex-1 opacity-80">
            {data && data.blockedSeries.length > 0 && (
              <Sparkline values={data.blockedSeries} color="var(--color-warn)" area={false} />
            )}
          </div>
        </div>
      </Card>
    </Link>
  );
}
