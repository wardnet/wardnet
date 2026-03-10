import { Star } from "lucide-react";
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
 * Content is driven by content/docs.yml — no code changes needed to update.
 */
export function Docs() {
  return (
    <div className="min-h-screen bg-white dark:bg-[oklch(0.13_0.02_270)]">
      <Navbar showBack />

      <main className="px-6 py-16">
        <div className="mx-auto max-w-4xl">
          <h1 className="mb-3 text-4xl font-bold text-gray-900 dark:text-gray-100">
            Documentation
          </h1>
          <p className="mb-12 text-lg text-gray-500 dark:text-gray-400">
            Guides and references for setting up and managing your Wardnet gateway. Documentation is
            being written — check back soon.
          </p>

          <div className="mb-12">
            <div className="mb-4 flex items-center gap-2 text-sm font-semibold uppercase tracking-wider text-[var(--brand-green)]">
              <Star size={14} />
              Recommended
            </div>
            <div className="flex flex-col gap-4">
              {recommended.map((entry) => {
                const Icon = resolveIcon(entry.icon);
                return (
                  <div
                    key={entry.slug}
                    className="rounded-lg border border-[var(--brand-green)]/20 bg-[var(--brand-green)]/5 p-5"
                  >
                    <div className="mb-2 flex items-center gap-3">
                      {Icon && (
                        <span className="text-[var(--brand-green)]">
                          <Icon size={20} />
                        </span>
                      )}
                      <h3 className="font-semibold text-gray-900 dark:text-gray-100">
                        {entry.title}
                      </h3>
                    </div>
                    <p className="text-sm leading-relaxed text-gray-500 dark:text-gray-400">
                      {entry.excerpt}
                    </p>
                  </div>
                );
              })}
            </div>
          </div>

          <div>
            <h2 className="mb-6 text-2xl font-bold text-gray-900 dark:text-gray-100">All topics</h2>
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              {topics.map((topic) => {
                const Icon = resolveIcon(topic.icon);
                return (
                  <div
                    key={topic.slug}
                    className="rounded-lg border border-gray-200 p-5 dark:border-gray-800"
                  >
                    <div className="mb-2 flex items-center gap-3">
                      {Icon && (
                        <span className="text-[var(--brand-green)]">
                          <Icon size={20} />
                        </span>
                      )}
                      <h3 className="font-semibold text-gray-900 dark:text-gray-100">
                        {topic.title}
                      </h3>
                    </div>
                    <p className="text-sm leading-relaxed text-gray-500 dark:text-gray-400">
                      {topic.description}
                    </p>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </main>
    </div>
  );
}
