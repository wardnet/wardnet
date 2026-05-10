import type { ReactNode } from "react";

interface PageHeaderProps {
  title: string;
  actions?: ReactNode;
}

/** Page title bar with optional action buttons on the right. */
export function PageHeader({ title, actions }: PageHeaderProps) {
  return (
    <div className="row mb-6 justify-between">
      <h2 className="h-title">{title}</h2>
      {actions && <div className="row gap-8">{actions}</div>}
    </div>
  );
}
