import { ArrowLeft } from "lucide-react";
import { Link, useNavigate } from "react-router";
import { Logo } from "@/components/compound/Logo";

interface NavbarProps {
  /** Callback fired when the logo/brand is clicked. Falls back to navigating to "/". */
  onLogoClick?: () => void;
  /** When true, shows a back arrow before the logo. */
  showBack?: boolean;
  /**
   * Fallback destination for the back arrow when there is no browser
   * history to pop (e.g. the page was opened directly via a deep link or
   * a shared URL). Defaults to the home content view. Article pages pass
   * `/docs` so a cold-loaded article still lands on the docs index rather
   * than the hero.
   */
  backTo?: string;
}

/**
 * Sticky top navigation. Rendered as Ward Navy chrome on `--color-side`
 * (Forge sidebar token), with no border or shadow below, the page surface
 * runs straight under it. Type and ink follow the Forge scale: `text-sm`
 * with the `side-ink` family for foreground.
 *
 * The back arrow (shown on sub-pages via `showBack`) pops real browser
 * history so it returns to wherever the user actually came from, whether
 * that's a homepage feature card or the docs index. The logo is a separate
 * link to home so the two affordances never disagree.
 */
export function Navbar({ onLogoClick, showBack, backTo }: NavbarProps) {
  const navigate = useNavigate();

  const handleBack = () => {
    // Prefer real history so "back" returns to the actual previous page.
    // When there's nothing to pop (cold deep link), fall back to a sensible
    // in-site destination instead of leaving the app.
    if (window.history.length > 1) {
      navigate(-1);
    } else {
      navigate(backTo ?? "/?view=content");
    }
  };

  const logo = <Logo height={28} variant="dark" />;

  return (
    <nav className="bg-side sticky top-0 z-50 flex w-full items-center justify-between px-6 py-4">
      <div className="flex items-center gap-2">
        {showBack && (
          <button
            onClick={handleBack}
            className="text-side-ink-2 hover:text-side-ink-active transition-colors"
            aria-label="Go back"
          >
            <ArrowLeft size={20} />
          </button>
        )}
        {onLogoClick ? (
          <button
            onClick={onLogoClick}
            className="flex items-center gap-2"
            aria-label="Wardnet home"
          >
            {logo}
          </button>
        ) : (
          <Link to="/" className="flex items-center gap-2" aria-label="Wardnet home">
            {logo}
          </Link>
        )}
      </div>
      <div className="flex items-center gap-6">
        <Link
          to="/premium"
          className="text-side-ink hover:text-side-ink-active t-size-sm t-weight-medium transition-colors"
        >
          Premium
        </Link>
        <Link
          to="/blog"
          className="text-side-ink hover:text-side-ink-active t-size-sm t-weight-medium transition-colors"
        >
          Blog
        </Link>
        <Link
          to="/docs"
          className="text-side-ink hover:text-side-ink-active t-size-sm t-weight-medium transition-colors"
        >
          Documentation
        </Link>
        <a
          href="https://github.com/wardnet/wardnet"
          className="text-side-ink hover:text-side-ink-active transition-colors"
          aria-label="GitHub"
        >
          <svg viewBox="0 0 16 16" className="h-5 w-5 fill-current" aria-hidden="true">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
          </svg>
        </a>
      </div>
    </nav>
  );
}
