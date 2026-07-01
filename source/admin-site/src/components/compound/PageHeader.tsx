import type { ReactNode } from "react";
import { Heading, Text } from "@wardnet/web";

interface PageHeaderProps {
  title: string;
  /** Optional one-line description rendered as `.h-sub` under the title. */
  description?: ReactNode;
  actions?: ReactNode;
}

/** Page title bar with optional description and action buttons. */
export function PageHeader({ title, description, actions }: PageHeaderProps) {
  return (
    <div className="row mb-6 justify-between">
      <div>
        <Heading
          level={2}
          size="3xl"
          className="text-ink"
          data-testid="page-title"
        >
          {title}
        </Heading>
        {description && (
          <Text as="p" size="sm" className="mt-1 text-ink-3">
            {description}
          </Text>
        )}
      </div>
      {actions && <div className="row gap-8">{actions}</div>}
    </div>
  );
}
