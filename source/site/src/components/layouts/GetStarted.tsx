import { CodeBlock } from "@/components/compound/CodeBlock";

/**
 * Quick-start section with an install command and a link to the GitHub repository.
 */
export function GetStarted() {
  return (
    <section className="bg-gray-50 px-6 py-20 dark:bg-[oklch(0.15_0.02_270)]">
      <div className="mx-auto max-w-2xl text-center">
        <h2 className="mb-8 text-3xl font-bold text-gray-900 dark:text-gray-100">Get started</h2>
        <CodeBlock code="curl -sSL https://wardnet.dev/install.sh | bash" />
        <p className="mt-6 text-sm text-gray-500 dark:text-gray-400">
          Or clone from GitHub and build from source.
        </p>
        <a
          href="https://github.com/pedromvgomes/wardnet"
          className="mt-4 inline-block text-sm font-medium text-[var(--brand-green)] hover:underline"
        >
          View on GitHub
        </a>
      </div>
    </section>
  );
}
