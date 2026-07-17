import { Newspaper } from "lucide-react";
import { Link } from "react-router";
import { Navbar } from "@/components/layouts/Navbar";
import blogContent from "../../content/blog.yml";

interface PostEntry {
  slug: string;
  title: string;
  date: string;
  description: string;
}

const posts = blogContent.posts as PostEntry[];

/** Format an ISO `YYYY-MM-DD` as e.g. "July 17, 2026". Parsed as UTC noon so
 *  the calendar day never shifts under a negative timezone offset. */
function formatDate(iso: string): string {
  const d = new Date(`${iso}T12:00:00Z`);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

/**
 * Blog index. Content is driven by content/blog.yml (one entry per post),
 * mirroring how content/docs.yml drives the docs catalogue — no code change
 * needed to publish a post.
 */
export function Blog() {
  return (
    <div className="min-h-screen bg-bg">
      <Navbar showBack />

      <main className="px-6 py-16">
        <div className="mx-auto max-w-4xl">
          <h1 className="mb-3 t-size-4xl t-weight-bold tracking-tight text-ink">Blog</h1>
          <p className="mb-12 t-size-lg text-ink-3">
            Notes on why Wardnet exists and how it works.
          </p>

          <div className="flex flex-col gap-4">
            {posts.map((post) => (
              <Link
                key={post.slug}
                to={`/blog/${post.slug}`}
                className="block rounded-lg border border-line bg-card p-5 transition-colors hover:border-line-strong hover:bg-elev"
              >
                <div className="mb-2 flex items-center gap-3">
                  <span className="text-ink-3">
                    <Newspaper size={20} />
                  </span>
                  <h3 className="t-weight-semibold text-ink">{post.title}</h3>
                </div>
                <p className="mb-2 t-size-sm leading-relaxed text-ink-3">{post.description}</p>
                <time dateTime={post.date} className="t-size-xs text-ink-3">
                  {formatDate(post.date)}
                </time>
              </Link>
            ))}
          </div>
        </div>
      </main>
    </div>
  );
}
