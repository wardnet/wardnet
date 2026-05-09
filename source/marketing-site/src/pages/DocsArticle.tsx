import { useParams } from "react-router";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Navbar } from "@/components/layouts/Navbar";
import docsContent from "../../content/docs.yml";

// Map each markdown element to site-consistent Tailwind classes. Keeps us
// free of a full typography plugin for a handful of docs pages.
const MD_COMPONENTS: Components = {
  h1: (props) => (
    <h1 className="mb-6 mt-0 text-4xl font-bold text-gray-900 dark:text-gray-100" {...props} />
  ),
  h2: (props) => (
    <h2 className="mb-4 mt-10 text-2xl font-bold text-gray-900 dark:text-gray-100" {...props} />
  ),
  h3: (props) => (
    <h3 className="mb-3 mt-6 text-lg font-semibold text-gray-900 dark:text-gray-100" {...props} />
  ),
  p: (props) => <p className="mb-4 leading-relaxed text-gray-600 dark:text-gray-300" {...props} />,
  ul: (props) => <ul className="mb-4 list-disc space-y-1 pl-6" {...props} />,
  ol: (props) => <ol className="mb-4 list-decimal space-y-1 pl-6" {...props} />,
  li: (props) => <li className="text-gray-600 dark:text-gray-300" {...props} />,
  a: (props) => <a className="font-medium text-[var(--brand-green)] hover:underline" {...props} />,
  code: ({ children, className }) => {
    // Inline code has no language class; fenced blocks get a `language-*`.
    const isInline = !className;
    if (isInline) {
      return (
        <code className="rounded bg-gray-100 px-1.5 py-0.5 font-mono text-sm text-gray-900 dark:bg-[oklch(0.18_0.02_270)] dark:text-gray-100">
          {children}
        </code>
      );
    }
    return <code className={className}>{children}</code>;
  },
  pre: (props) => (
    <pre
      className="mb-4 overflow-x-auto rounded-lg bg-gray-900 p-4 font-mono text-sm leading-relaxed text-gray-100"
      {...props}
    />
  ),
  blockquote: (props) => (
    <blockquote
      className="mb-4 border-l-4 border-[var(--brand-green)]/40 pl-4 italic text-gray-500 dark:text-gray-400"
      {...props}
    />
  ),
  table: (props) => (
    <div className="mb-4 overflow-x-auto">
      <table
        className="w-full border-collapse text-sm text-gray-600 dark:text-gray-300"
        {...props}
      />
    </div>
  ),
  th: (props) => (
    <th
      className="border-b border-gray-300 px-3 py-2 text-left font-semibold text-gray-900 dark:border-gray-700 dark:text-gray-100"
      {...props}
    />
  ),
  td: (props) => (
    <td className="border-b border-gray-200 px-3 py-2 dark:border-gray-800" {...props} />
  ),
  hr: () => <hr className="my-8 border-gray-200 dark:border-gray-800" />,
  strong: (props) => (
    <strong className="font-semibold text-gray-900 dark:text-gray-100" {...props} />
  ),
  // Screenshots are captured at retina density (~2x) which makes the raw
  // pixel dimensions overwhelm our `max-w-[72rem]` article column. Cap
  // them at a readable width and centre them so every screenshot reads
  // as a focused illustration rather than a full-bleed hero image.
  //
  // A few screenshots (wide cards, banners) read better when they fill
  // the whole column. Opt in by setting the markdown title:
  //
  //   ![alt](path "wide")   → no width cap
  //
  // Default (no title) stays at `max-w-2xl` — the right size for
  // dialog / modal screenshots.
  img: ({ title, ...props }) => {
    const wide = title === "wide";
    return (
      <img
        className={
          "my-6 mx-auto block w-full rounded-lg border border-gray-200 dark:border-gray-800" +
          (wide ? "" : " max-w-2xl")
        }
        loading="lazy"
        // The `title` was a sizing directive — don't let it leak
        // into the rendered HTML as a tooltip.
        {...props}
      />
    );
  },
};

/**
 * Eager-load every markdown file under `content/docs/*.md` at build time as
 * raw strings. `import.meta.glob` returns a record keyed by the path Vite
 * resolved, so we normalise to slug → content at module load — one lookup
 * per page render.
 */
const DOC_MODULES = import.meta.glob("../../content/docs/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const DOCS_BY_SLUG: Record<string, string> = Object.fromEntries(
  Object.entries(DOC_MODULES).map(([path, body]) => {
    const match = path.match(/\/([^/]+)\.md$/);
    return [match ? match[1] : path, body];
  }),
);

interface TopicEntry {
  slug: string;
  title: string;
  description: string;
}

const topics = docsContent.topics as TopicEntry[];

/**
 * Renders a single documentation article from a markdown file under
 * `content/docs/<slug>.md`. Slugs that don't have a file yet render a
 * "coming soon" placeholder so the links in the docs catalogue are never
 * broken.
 */
export function DocsArticle() {
  const { slug = "" } = useParams();
  const topic = topics.find((t) => t.slug === slug);
  const body = DOCS_BY_SLUG[slug];
  const title = topic?.title ?? slug;

  return (
    <div className="min-h-screen bg-white dark:bg-[oklch(0.13_0.02_270)]">
      <Navbar showBack backTo="/docs" />

      <main className="px-6 py-16">
        {/* Use the full viewport width with a high cap so the docs fill large
            screens. 72rem stops lines from becoming an unreadable sea of
            text while still using noticeably more horizontal space than the
            old 3xl cap. */}
        <div className="mx-auto max-w-[72rem]">
          {body ? (
            <article>
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
                {body}
              </ReactMarkdown>
            </article>
          ) : (
            <ComingSoon title={title} description={topic?.description} />
          )}
        </div>
      </main>
    </div>
  );
}

function ComingSoon({ title, description }: { title: string; description?: string }) {
  return (
    <div className="rounded-lg border border-dashed border-gray-300 bg-gray-50 p-10 text-center dark:border-gray-700 dark:bg-[oklch(0.15_0.02_270)]">
      <h1 className="mb-2 text-3xl font-bold text-gray-900 dark:text-gray-100">{title}</h1>
      {description && (
        <p className="mb-6 text-sm text-gray-500 dark:text-gray-400">{description}</p>
      )}
      <p className="text-sm font-medium text-[var(--brand-green)]">Documentation coming soon.</p>
    </div>
  );
}
