import { TechBadge } from "@/components/compound/TechBadge";

const TECHNOLOGIES = [
  "Rust",
  "React",
  "TypeScript",
  "WireGuard",
  "SQLite",
  "Tailwind CSS",
  "Raspberry Pi",
] as const;

/**
 * Section displaying technology badges for the tools and frameworks used in Wardnet.
 */
export function TechStack() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl text-center">
        <h2 className="mb-10 text-3xl font-bold text-gray-900 dark:text-gray-100">
          Built with modern tools
        </h2>
        <div className="flex flex-wrap justify-center gap-3">
          {TECHNOLOGIES.map((tech) => (
            <TechBadge key={tech} label={tech} />
          ))}
        </div>
      </div>
    </section>
  );
}
