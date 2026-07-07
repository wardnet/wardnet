import { useParams } from "react-router";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Navbar } from "@/components/layouts/Navbar";
import { MD_COMPONENTS } from "@/lib/markdown-components";
import docsContent from "../../content/docs.yml";

/**
 * Eager-load every markdown file under `content/docs/*.md` at build time as
 * raw strings. `import.meta.glob` returns a record keyed by the path Vite
 * resolved, so we normalise to slug → content at module load, one lookup
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
    <div className="min-h-screen bg-bg">
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
    <div className="empty">
      <h1 className="h-title">{title}</h1>
      {description && <p className="h-sub max-w-md mx-auto">{description}</p>}
      <p className="mt-6 t-size-sm t-weight-medium text-accent">Documentation coming soon.</p>
    </div>
  );
}
