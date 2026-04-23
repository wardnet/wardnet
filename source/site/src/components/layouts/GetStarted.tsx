import { CodeBlock } from "@/components/compound/CodeBlock";
import { LatestReleaseBadge } from "@/components/compound/LatestReleaseBadge";

const DOCKER_RUN = `docker run -d \\
  --name wardnetd \\
  --cap-add NET_ADMIN --cap-add NET_RAW \\
  --device /dev/net/tun \\
  --sysctl net.ipv4.ip_forward=1 \\
  --tmpfs /run --tmpfs /run/lock \\
  -p 7411:7411 \\
  -v wardnet-data:/var/lib/wardnet \\
  ghcr.io/wardnet/wardnetd:latest`;

/**
 * Quick-start section with Docker (recommended) and bare-metal install options.
 */
export function GetStarted() {
  return (
    <section className="bg-gray-50 px-6 py-20 dark:bg-[oklch(0.15_0.02_270)]">
      <div className="mx-auto max-w-2xl text-center">
        <h2 className="mb-4 text-3xl font-bold text-gray-900 dark:text-gray-100">Get started</h2>
        <div className="mb-6 flex justify-center">
          <LatestReleaseBadge />
        </div>

        <p className="mb-2 text-left text-xs font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">
          Run with Docker
        </p>
        <CodeBlock code={DOCKER_RUN} className="text-left" />

        <div className="my-6 flex items-center gap-3">
          <hr className="flex-1 border-gray-300 dark:border-gray-700" />
          <span className="text-xs text-gray-400 dark:text-gray-500">or</span>
          <hr className="flex-1 border-gray-300 dark:border-gray-700" />
        </div>

        <p className="mb-2 text-left text-xs font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">
          Bare-metal install
        </p>
        <CodeBlock code="curl -sSL https://wardnet.network/install.sh | sudo bash" className="text-left" />

        <p className="mt-6 text-sm text-gray-500 dark:text-gray-400">
          See the{" "}
          <a href="/docs/installation" className="text-[var(--brand-green)] hover:underline">
            installation guide
          </a>{" "}
          for compose options, air-gapped installs, and channel selection.
        </p>
        <a
          href="https://github.com/wardnet/wardnet"
          className="mt-4 inline-block text-sm font-medium text-[var(--brand-green)] hover:underline"
        >
          View on GitHub
        </a>
      </div>
    </section>
  );
}
