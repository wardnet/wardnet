import { Link } from "react-router";
import { Logo } from "@/components/compound/Logo";

const EXTERNAL_LINKS = [
  { label: "GitHub", href: "https://github.com/pedromvgomes/wardnet" },
  {
    label: "Releases",
    href: "https://github.com/pedromvgomes/wardnet/releases",
  },
  {
    label: "MIT License",
    href: "https://github.com/pedromvgomes/wardnet/blob/main/LICENSE",
  },
] as const;

/**
 * Site footer with the Wardnet logo, navigation links, and copyright notice.
 */
export function Footer() {
  return (
    <footer className="border-t border-gray-200 px-6 py-12 dark:border-gray-800">
      <div className="mx-auto flex max-w-6xl flex-col items-center gap-6">
        <div className="flex items-center gap-2">
          <Logo size={32} />
          <span className="text-lg font-semibold text-gray-900 dark:text-gray-100">Wardnet</span>
        </div>
        <nav className="flex gap-6">
          <Link
            to="/docs"
            className="text-sm text-gray-500 transition-colors hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-200"
          >
            Documentation
          </Link>
          {EXTERNAL_LINKS.map((link) => (
            <a
              key={link.label}
              href={link.href}
              className="text-sm text-gray-500 transition-colors hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-200"
            >
              {link.label}
            </a>
          ))}
        </nav>
        <p className="text-xs text-gray-400 dark:text-gray-500">
          MIT License. Built with Rust and React.
        </p>
      </div>
    </footer>
  );
}
