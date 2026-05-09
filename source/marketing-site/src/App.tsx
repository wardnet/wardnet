import { BrowserRouter, Routes, Route } from "react-router";
import { ErrorBoundary } from "@/components/compound/ErrorBoundary";
import { Home } from "@/pages/Home";
import { Docs } from "@/pages/Docs";
import { DocsArticle } from "@/pages/DocsArticle";
import { NotFound } from "@/pages/NotFound";

/**
 * Root application component with routing for the public site.
 *
 * Wrapped in [`ErrorBoundary`] so any uncaught render error bubbles up to
 * a styled fallback instead of showing the browser's default crash view.
 * The trailing `path="*"` route catches unknown URLs — GitHub Pages is
 * configured (via `cp index.html 404.html` at build time) to serve the
 * SPA for any path, so this component is what the user actually sees for
 * non-existent routes.
 */
export default function App() {
  return (
    <ErrorBoundary>
      <BrowserRouter basename={import.meta.env.BASE_URL}>
        <Routes>
          <Route path="/" element={<Home />} />
          <Route path="/docs" element={<Docs />} />
          <Route path="/docs/:slug" element={<DocsArticle />} />
          {/* Dev-only: force a render-time error so the ErrorBoundary can be
              exercised locally. Stripped from production builds by Vite's
              dead-code elimination on `import.meta.env.DEV`. */}
          {import.meta.env.DEV && <Route path="/throw" element={<ThrowOnRender />} />}
          <Route path="*" element={<NotFound />} />
        </Routes>
      </BrowserRouter>
    </ErrorBoundary>
  );
}

function ThrowOnRender(): never {
  throw new Error("forced error from /throw (dev only)");
}
