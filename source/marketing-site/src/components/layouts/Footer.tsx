import { Link } from "react-router";
import { Logo } from "@/components/compound/Logo";

const EXTERNAL_LINKS = [
  { label: "GitHub", href: "https://github.com/wardnet/wardnet" },
  {
    label: "Releases",
    href: "https://github.com/wardnet/wardnet/releases",
  },
  {
    label: "MIT License",
    href: "https://github.com/wardnet/wardnet/blob/main/LICENSE",
  },
] as const;

/**
 * Site footer with the Wardnet logo, navigation links, and copyright notice.
 */
export function Footer() {
  return (
    <footer className="border-t border-line px-6 py-12">
      <div className="mx-auto flex max-w-6xl flex-col items-center gap-6">
        <div className="flex items-center gap-2">
          <Logo size={32} />
          <span className="text-lg font-semibold text-ink">
            Ward<span style={{ color: "var(--accent)" }}>net</span>
          </span>
        </div>
        <nav className="flex gap-6">
          <Link to="/docs" className="text-sm text-ink-3 transition-colors hover:text-ink">
            Documentation
          </Link>
          {EXTERNAL_LINKS.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="text-sm text-ink-3 transition-colors hover:text-ink"
            >
              {link.label}
            </a>
          ))}
        </nav>
        <p className="text-xs text-ink-4">MIT License. Built with Rust and React.</p>
      </div>
    </footer>
  );
}
