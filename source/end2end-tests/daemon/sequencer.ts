import { BaseSequencer, type TestSpecification } from "vitest/node";

/**
 * Spec files that mutate state the rest of the suite depends on, and must
 * therefore run last. Matched as a substring of the file path.
 *
 * Most specs share the daemon but leave the clients where they found them.
 * These do not: they move a device between zones, which changes the DHCP scope
 * it leases from, so the client comes back on a different base-pool address.
 *
 * That matters because the device-oriented specs identify their subject by IP
 * *range* (`findDeviceByIpRangeOrNull` over 10.91.0.100-150), not by MAC. The
 * lookup is only sound while each client keeps a stable base-pool lease. Once a
 * lease is reshuffled, the range can resolve to a *different* device than the
 * one the spec is actually driving — the filter is applied to one device while
 * the DNS queries come from another, and the spec fails with the
 * indistinguishable-looking "expected 0 to be greater than 0".
 *
 * Deferring these files confines that disturbance to the end of the run. It is
 * confinement, not elimination: the underlying fragility is the range lookup,
 * and moving it to a MAC-keyed lookup would remove the constraint entirely.
 */
const DEFERRED = ["zone-host-route-prefsrc"];

function isDeferred(spec: TestSpecification): boolean {
  return DEFERRED.some((name) => spec.moduleId.includes(name));
}

/**
 * Keeps vitest's own ordering (size descending, so the slowest files start
 * first) and then moves the deferred specs to the end.
 *
 * Without this the order is size-descending only, which is not alphabetical and
 * gives no stable position to a disruptive file — `zone-host-route-prefsrc` is
 * the 4th largest of 34, so it ran 4th and its churn preceded 30 other files.
 */
export default class DeferDisruptiveSequencer extends BaseSequencer {
  async sort(files: TestSpecification[]): Promise<TestSpecification[]> {
    const sorted = await super.sort(files);
    return [...sorted.filter((f) => !isDeferred(f)), ...sorted.filter(isDeferred)];
  }
}
