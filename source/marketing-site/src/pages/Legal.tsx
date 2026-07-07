import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Navbar } from "@/components/layouts/Navbar";
import { MD_COMPONENTS } from "@/lib/markdown-components";

/**
 * Eager-load every markdown file under `content/legal/*.md` at build time,
 * the same `import.meta.glob` pattern DocsArticle.tsx uses for
 * `content/docs/*.md`.
 */
const LEGAL_MODULES = import.meta.glob("../../content/legal/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const LEGAL_BY_SLUG: Record<string, string> = Object.fromEntries(
  Object.entries(LEGAL_MODULES).map(([path, body]) => {
    const match = path.match(/\/([^/]+)\.md$/);
    return [match ? match[1] : path, body];
  }),
);

/** Renders a static legal page (Terms of Service, Privacy Policy) from `content/legal/<slug>.md`. */
export function Legal({ slug }: { slug: "terms" | "privacy" }) {
  const body = LEGAL_BY_SLUG[slug];

  return (
    <div className="min-h-screen bg-bg">
      <Navbar showBack backTo="/" />

      <main className="px-6 py-16">
        <div className="mx-auto max-w-[72rem]">
          <article>
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
              {body}
            </ReactMarkdown>
          </article>
        </div>
      </main>
    </div>
  );
}
