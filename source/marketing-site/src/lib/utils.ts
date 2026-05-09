/**
 * Concatenates CSS class names, filtering out falsy values.
 * Lightweight alternative to clsx + tailwind-merge.
 */
export function cn(...classes: (string | undefined | false | null)[]): string {
  return classes.filter(Boolean).join(" ");
}
