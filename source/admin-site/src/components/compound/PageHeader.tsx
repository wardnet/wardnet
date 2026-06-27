import type { ReactNode } from "react";

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
        <h2 className="h-title" data-testid="page-title">
          {title}
        </h2>
        {description && <p className="h-sub">{description}</p>}
      </div>
      {actions && <div className="row gap-8">{actions}</div>}
    </div>
  );
}
