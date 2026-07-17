import { useParams } from "react-router";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Navbar } from "@/components/layouts/Navbar";
import { MD_COMPONENTS } from "@/lib/markdown-components";
import blogContent from "../../content/blog.yml";

/**
 * Eager-load every markdown file under `content/blog/*.md` at build time as
 * raw strings, then normalise to slug → content, mirroring DocsArticle.
 */
const BLOG_MODULES = import.meta.glob("../../content/blog/*.md", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const BLOG_BY_SLUG: Record<string, string> = Object.fromEntries(
  Object.entries(BLOG_MODULES).map(([path, body]) => {
    const match = path.match(/\/([^/]+)\.md$/);
    return [match ? match[1] : path, body];
  }),
);

interface PostEntry {
  slug: string;
  title: string;
  date: string;
  description: string;
}

const posts = blogContent.posts as PostEntry[];

/**
 * Renders a single blog post from `content/blog/<slug>.md`. Slugs without a
 * file fall back to a "coming soon" placeholder so links never break.
 */
export function BlogArticle() {
  const { slug = "" } = useParams();
  const post = posts.find((p) => p.slug === slug);
  // eslint-disable-next-line security/detect-object-injection -- read-only index into a build-time map of bundled markdown; slug from URL cannot write or reach the FS
  const body = BLOG_BY_SLUG[slug];
  const title = post?.title ?? slug;

  return (
    <div className="min-h-screen bg-bg">
      <Navbar showBack backTo="/blog" />

      <main className="px-6 py-16">
        {/* Same readable cap as DocsArticle so prose and screenshots share a
            column width across the two sections. */}
        <div className="mx-auto max-w-[72rem]">
          {body ? (
            <article>
              <ReactMarkdown remarkPlugins={[remarkGfm]} components={MD_COMPONENTS}>
                {body}
              </ReactMarkdown>
            </article>
          ) : (
            <div className="empty">
              <h1 className="h-title">{title}</h1>
              <p className="mt-6 t-size-sm t-weight-medium text-accent">Post coming soon.</p>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}
