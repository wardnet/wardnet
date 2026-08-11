import { Link } from "react-router";
import { Pill, Text, anomalySubjectLink, useAnomalies } from "@wardnet/web";
import { ChevronRightIcon } from "lucide-react";

import { SectionLabel } from "@/components/SectionLabel";
import { ADMIN_APP_ANOMALY_ROUTES } from "@/lib/anomalyRoutes";

function severityVariant(severity: string): "danger" | "warn" | "info" {
  if (severity === "error") return "danger";
  if (severity === "warning") return "warn";
  return "info";
}

/**
 * The conditions currently open on the box.
 *
 * Renders nothing when everything is healthy: a permanently-present "no
 * problems" block on a phone screen costs more than it says. Links are
 * coarser here than on the desktop admin site — this app has section routes,
 * not detail pages — which the shared resolver handles by falling back.
 */
export function AnomaliesList() {
  const { data } = useAnomalies();
  const anomalies = data?.anomalies ?? [];

  if (anomalies.length === 0) return null;

  return (
    <div>
      <SectionLabel>Anomalies</SectionLabel>
      <div
        className="overflow-hidden rounded-xl border border-line bg-card"
        data-testid="system-anomalies"
      >
        <ul className="divide-y divide-line">
          {anomalies.map((anomaly) => {
            const link = anomalySubjectLink(anomaly, ADMIN_APP_ANOMALY_ROUTES);
            const row = (
              <>
                <div className="mt-0.5 shrink-0">
                  <Pill variant={severityVariant(anomaly.severity)}>
                    {anomaly.component}
                  </Pill>
                </div>
                <div className="min-w-0 flex-1">
                  <Text as="p" size="sm" weight="semibold" className="text-ink">
                    {anomaly.message}
                  </Text>
                  {anomaly.hint && (
                    <Text as="p" size="xs" className="text-ink-3">
                      {anomaly.hint}
                    </Text>
                  )}
                </div>
                {link && (
                  <ChevronRightIcon
                    size={16}
                    aria-hidden
                    className="mt-0.5 shrink-0 text-ink-4"
                  />
                )}
              </>
            );

            return (
              <li key={anomaly.id} data-testid="system-anomaly-row">
                {link ? (
                  <Link
                    to={link.href}
                    aria-label={`${link.label}: ${anomaly.message}`}
                    className="flex items-start gap-3 px-4 py-3"
                  >
                    {row}
                  </Link>
                ) : (
                  <div className="flex items-start gap-3 px-4 py-3">{row}</div>
                )}
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
