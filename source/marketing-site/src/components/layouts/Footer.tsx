import { Link } from "react-router";
import { Logo } from "@/components/compound/Logo";

const EXTERNAL_LINKS = [
  { label: "GitHub", href: "https://github.com/wardnet/wardnet" },
  {
    label: "Releases",
    href: "https://github.com/wardnet/wardnet/releases",
  },
  {
    label: "License",
    href: "https://github.com/wardnet/wardnet/blob/main/LICENSING.md",
  },
  { label: "Status", href: "https://status.wardnet.network" },
] as const;

/**
 * Site footer with the Wardnet logo, navigation links, and copyright notice.
 */
export function Footer() {
  return (
    <footer className="border-t border-line px-6 py-12">
      <div className="mx-auto flex max-w-6xl flex-col items-center gap-6">
        <Logo height={32} variant="light" />
        <nav className="flex gap-6">
          <Link to="/premium" className="t-size-sm text-ink-3 transition-colors hover:text-ink">
            Premium
          </Link>
          <Link to="/docs" className="t-size-sm text-ink-3 transition-colors hover:text-ink">
            Documentation
          </Link>
          <Link to="/terms" className="t-size-sm text-ink-3 transition-colors hover:text-ink">
            Terms
          </Link>
          <Link to="/privacy" className="t-size-sm text-ink-3 transition-colors hover:text-ink">
            Privacy
          </Link>
          {EXTERNAL_LINKS.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="t-size-sm text-ink-3 transition-colors hover:text-ink"
            >
              {link.label}
            </a>
          ))}
        </nav>
        <p className="t-size-xs text-ink-4">
          GPL-3.0-or-later (daemon), MIT (SDK). Built with Rust and React.
        </p>
      </div>
    </footer>
  );
}
