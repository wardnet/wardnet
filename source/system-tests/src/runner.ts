import { env } from "./helpers/env.js";

import { steps as healthSteps } from "./tests/01-health.js";
import { steps as tunnelImportSteps } from "./tests/02-tunnel-import.js";
import { steps as deviceDetectionSteps } from "./tests/03-device-detection.js";
import { steps as deviceRoutingSteps } from "./tests/04-device-routing.js";
import { steps as trafficRoutingSteps } from "./tests/05-traffic-routing.js";
import { steps as multiTunnelSteps } from "./tests/06-multi-tunnel.js";
import { steps as idleTeardownSteps } from "./tests/07-idle-teardown.js";

export type Step = [name: string, fn: () => Promise<void>];

const allSteps: Step[] = [
  ...healthSteps,
  ...tunnelImportSteps,
  ...deviceDetectionSteps,
  ...deviceRoutingSteps,
  ...trafficRoutingSteps,
  ...multiTunnelSteps,
  ...idleTeardownSteps,
];

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// Re-export sleep for test files.
export { sleep };

async function run() {
  const startTime = Date.now();
  let passed = 0;
  let failed = 0;

  console.log(`\n  Wardnet System Tests\n`);
  console.log(`  PI: ${env.piHost}  API: ${env.apiUrl}  Agent: ${env.agentUrl}\n`);

  for (const [name, fn] of allSteps) {
    const t0 = Date.now();
    try {
      await fn();
      const ms = Date.now() - t0;
      console.log(`  \x1b[32m✓\x1b[0m ${name} \x1b[90m(${ms}ms)\x1b[0m`);
      passed++;
    } catch (err) {
      const ms = Date.now() - t0;
      const msg = err instanceof Error ? err.message : String(err);
      console.log(`  \x1b[31m✗\x1b[0m ${name} \x1b[90m(${ms}ms)\x1b[0m`);
      console.log(`    \x1b[31m${msg}\x1b[0m\n`);
      failed++;
      break;
    }
  }

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  const skipped = allSteps.length - passed - failed;
  console.log(`\n  ${passed} passed, ${failed} failed, ${skipped} skipped (${elapsed}s)\n`);
  process.exit(failed > 0 ? 1 : 0);
}

run();
