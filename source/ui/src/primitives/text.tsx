import * as React from "react";
import { clsx } from "clsx";

/**
 * <Text> — the single typography primitive for the design system.
 *
 * A `role` is the common case: it bakes a default for every typographic
 * property (size + weight + colour + the rendered element). Each default is
 * individually overridable via `size` / `weight` / `color` / `as`. Omitting
 * `role` yields plain text driven entirely by the override props.
 *
 * This component ships NO CSS — it references the @wardnet/styles role and
 * helper classes by string (`t-label`, `t-size-sm`, `t-weight-semibold`) and
 * mirrors the colour utilities for `color` (`text-danger`), keeping the
 * decision-4 cascade (utilities beat the `@layer components` role colour).
 * See docs/adr-typography-scale-and-roles.md.
 */

type Role =
  | "label"
  | "body"
  | "body-strong"
  | "caption"
  | "micro"
  | "metric"
  | "metric-unit"
  | "mono"
  | "h1"
  | "h2"
  | "h3";

type Size =
  | "2xs"
  | "xs"
  | "sm"
  | "base"
  | "lg"
  | "xl"
  | "2xl"
  | "3xl"
  | "4xl";

type Weight = "normal" | "medium" | "semibold" | "bold";

/** Colour tokens with a `text-*` utility in the @wardnet/styles theme. */
type Color =
  | "ink"
  | "ink-2"
  | "ink-3"
  | "ink-4"
  | "ink-5"
  | "accent"
  | "accent-ink"
  | "accent-soft-ink"
  | "warn"
  | "warn-soft-ink"
  | "danger"
  | "danger-soft-ink"
  | "info";

interface TextOwnProps {
  /** Named voice — a default bundle of size + weight + colour + element. */
  role?: Role;
  /** Override the role's size (also the knob for off-role one-offs). */
  size?: Size;
  /** Override the role's weight. */
  weight?: Weight;
  /** Override the role's colour; emits the matching `text-*` utility. */
  color?: Color;
  /** Override the role's default element. */
  as?: React.ElementType;
  className?: string;
  children?: React.ReactNode;
}

type TextProps = TextOwnProps &
  Omit<React.HTMLAttributes<HTMLElement>, keyof TextOwnProps | "color">;

/** The element a role renders as unless `as` overrides it. */
const ROLE_ELEMENT: Record<Role, React.ElementType> = {
  label: "span",
  body: "p",
  "body-strong": "span",
  caption: "p",
  micro: "span",
  metric: "span",
  "metric-unit": "span",
  mono: "span",
  h1: "h1",
  h2: "h2",
  h3: "h3",
};

function Text({
  role,
  size,
  weight,
  color,
  as,
  className,
  ...props
}: TextProps) {
  const Comp = as ?? (role ? ROLE_ELEMENT[role] : "span");
  return (
    <Comp
      className={clsx(
        role && `t-${role}`,
        size && `t-size-${size}`,
        weight && `t-weight-${weight}`,
        color && `text-${color}`,
        className,
      )}
      {...props}
    />
  );
}

/** <Heading level={n}> ≡ <Text role={`h${n}`}>. */
interface HeadingProps extends Omit<TextProps, "role"> {
  level: 1 | 2 | 3;
}

function Heading({ level, ...props }: HeadingProps) {
  return <Text role={`h${level}`} {...props} />;
}

export { Text, Heading };
export type { TextProps, HeadingProps, Role, Size, Weight, Color };
