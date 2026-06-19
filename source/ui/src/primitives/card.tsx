import * as React from "react";
import { clsx } from "clsx";
import s from "./card.module.css";

type CardProps = React.ComponentProps<"div"> & {
  /** When set, replaces children with a danger-toned error message. */
  error?: string | null;
};

function Card({ className, error, children, ...props }: CardProps) {
  return (
    <div data-slot="card" className={clsx(s.card, className)} {...props}>
      {error ? (
        <div
          data-slot="card-error"
          style={{
            display: "flex",
            alignItems: "flex-start",
            gap: "0.5rem",
            padding: "0.875rem 1rem",
            background: "var(--danger-soft)",
            color: "var(--danger-soft-ink)",
            borderRadius: "inherit",
          }}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            style={{
              width: "1rem",
              height: "1rem",
              flexShrink: 0,
              marginTop: "0.125rem",
            }}
            aria-hidden="true"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="12" x2="12" y1="8" y2="12" />
            <line x1="12" x2="12.01" y1="16" y2="16" />
          </svg>
          <p
            style={{
              fontSize: "0.8125rem",
              fontWeight: 500,
              margin: 0,
              lineHeight: "1.4",
            }}
          >
            {error}
          </p>
        </div>
      ) : (
        children
      )}
    </div>
  );
}

function CardHeader({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-header"
      className={clsx(s.head, className)}
      {...props}
    />
  );
}

type CardTitleProps = React.ComponentProps<"h3"> & {
  /** The rendered element. Defaults to <h3>, but the title voice (the shared
   *  `t-label` role) is decoupled from the element — pass `as` to fit the
   *  surrounding heading outline (e.g. `as="h2"`) or drop the heading entirely
   *  (`as="div"`). */
  as?: React.ElementType;
};

function CardTitle({ className, as: Comp = "h3", ...props }: CardTitleProps) {
  return (
    <Comp
      data-slot="card-title"
      className={clsx("t-label", className)}
      {...props}
    />
  );
}

function CardSubtitle({
  className,
  ...props
}: React.ComponentProps<"p">) {
  return (
    <p
      data-slot="card-subtitle"
      className={clsx(s.subtitle, className)}
      {...props}
    />
  );
}

function CardAction({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-action"
      className={clsx(s.right, className)}
      {...props}
    />
  );
}

function CardContent({ className, ...props }: React.ComponentProps<"div">) {
  return <div data-slot="card-content" className={className} {...props} />;
}

function CardFooter({ className, ...props }: React.ComponentProps<"div">) {
  return (
    <div
      data-slot="card-footer"
      className={clsx(s.foot, className)}
      {...props}
    />
  );
}

export {
  Card,
  CardHeader,
  CardTitle,
  CardSubtitle,
  CardAction,
  CardContent,
  CardFooter,
};
