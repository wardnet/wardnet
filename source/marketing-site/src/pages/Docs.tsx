import { Star } from "lucide-react";
import { Link } from "react-router";
import { Navbar } from "@/components/layouts/Navbar";
import { resolveIcon } from "@/lib/icons";
import docsContent from "../../content/docs.yml";

interface RecommendedEntry {
  slug: string;
  title: string;
  icon: string;
  excerpt: string;
}

interface TopicEntry {
  slug: string;
  title: string;
  icon: string;
  description: string;
}

const recommended = docsContent.recommended as RecommendedEntry[];
const topics = docsContent.topics as TopicEntry[];

/**
 * Documentation page with a recommended section and full topic listing.
 * Content is driven by content/docs.yml, no code changes needed to update.
 */
export function Docs() {
  return (
    <div className="min-h-screen bg-bg">
      <Navbar showBack />

      <main className="px-6 py-16">
        <div className="mx-auto max-w-4xl">
          <h1 className="mb-3 t-size-4xl t-weight-bold tracking-tight text-ink">Documentation</h1>
          <p className="mb-12 t-size-lg text-ink-3">
            Guides and references for setting up and managing your Wardnet gateway. Documentation is
            being written, check back soon.
          </p>

          <div className="mb-12">
            <div className="mb-4 flex items-center gap-2 t-size-sm t-weight-semibold uppercase tracking-wider text-accent">
              <Star size={14} />
              Recommended
            </div>
            <div className="flex flex-col gap-4">
              {recommended.map((entry) => {
                const Icon = resolveIcon(entry.icon);
                return (
                  <Link
                    key={entry.slug}
                    to={`/docs/${entry.slug}`}
                    className="block rounded-lg border border-accent/20 bg-accent-soft p-5 transition-colors hover:bg-accent/10"
                  >
                    <div className="mb-2 flex items-center gap-3">
                      {Icon && (
                        <span className="text-accent">
                          <Icon size={20} />
                        </span>
                      )}
                      <h3 className="t-weight-semibold text-ink">{entry.title}</h3>
                    </div>
                    <p className="t-size-sm leading-relaxed text-ink-3">{entry.excerpt}</p>
                  </Link>
                );
              })}
            </div>
          </div>

          <div>
            <h2 className="mb-6 t-size-2xl t-weight-bold tracking-tight text-ink">All topics</h2>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              {topics.map((topic) => {
                const Icon = resolveIcon(topic.icon);
                return (
                  <Link
                    key={topic.slug}
                    to={`/docs/${topic.slug}`}
                    className="block rounded-lg border border-line bg-card p-5 transition-colors hover:border-line-strong hover:bg-elev"
                  >
                    <div className="mb-2 flex items-center gap-3">
                      {Icon && (
                        <span className="text-accent">
                          <Icon size={20} />
                        </span>
                      )}
                      <h3 className="t-weight-semibold text-ink">{topic.title}</h3>
                    </div>
                    <p className="t-size-sm leading-relaxed text-ink-3">{topic.description}</p>
                  </Link>
                );
              })}
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
