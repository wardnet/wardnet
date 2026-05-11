import { StepCard } from "@/components/compound/StepCard";

const STEPS = [
  {
    step: 1,
    title: "Install on your gateway",
    description:
      "Run the install script on a Raspberry Pi, a mini-PC, or any Linux host with a spare NIC.",
  },
  {
    step: 2,
    title: "Connect your devices",
    description: "Point your devices to the gateway. They're automatically detected and protected.",
  },
  {
    step: 3,
    title: "Control from the dashboard",
    description: "Manage tunnels, routing rules, and device policies from the web UI.",
  },
] as const;

/**
 * Three-step overview explaining how to get started with Wardnet.
 */
export function HowItWorks() {
  return (
    <section className="bg-sunken px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <h2
          className="mb-12 text-center text-3xl font-bold tracking-tight"
          style={{ color: "var(--accent)" }}
        >
          Up and running in minutes
        </h2>
        <div className="grid grid-cols-1 gap-10 md:grid-cols-3">
          {STEPS.map((s) => (
            <StepCard key={s.step} step={s.step} title={s.title} description={s.description} />
          ))}
        </div>
      </div>
    </section>
  );
}
