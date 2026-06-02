import { GlobeIcon, ShieldOffIcon } from "lucide-react";
import { Link } from "react-router";
import { Card } from "@wardnet/forge-web/card";
import { Sparkline } from "@wardnet/forge-web/sparkline";
import type { DashboardDnsStats } from "@wardnet/wardnet-web";

interface Props {
  data: DashboardDnsStats | undefined;
  isLoading: boolean;
}

export function DnsQueriesCard({ data, isLoading }: Props) {
  return (
    <Link to="/dns" className="block">
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
            <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              DNS Queries · 24h
            </div>
            <div className="mt-0.5 text-2xl font-bold text-ink tabular-nums">
              {isLoading && !data ? "…" : (data?.total.toLocaleString() ?? "0")}
            </div>
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
    <Link to="/dns" className="block">
      <Card className="card--flush">
        <div className="flex items-center gap-3 px-2 py-3">
          <div
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style={{ background: "var(--color-warn-soft)", color: "var(--color-warn)" }}
          >
            <ShieldOffIcon size={20} strokeWidth={1.8} />
          </div>

          <div className="min-w-0 shrink-0">
            <div className="text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              Blocked · 24h
            </div>
            <div className="mt-0.5 flex items-baseline gap-0.5">
              <span className="text-2xl font-bold text-ink tabular-nums">
                {isLoading && !data ? "…" : `${(data?.blockedPercent ?? 0).toFixed(1)}`}
              </span>
              <span className="text-sm text-ink-3">%</span>
            </div>
            {sub && <div className="mt-0.5 text-xs text-ink-3">{sub}</div>}
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
