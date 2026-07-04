import type { WizardStep } from "@wardnet/js";

/**
 * Canonical wizard step order and rail labels — the single source of
 * truth for the admin-site. Mirrors `WizardStep::ordinal()` in
 * `wardnet-common/src/api.rs`; the daemon validates transitions, this
 * registry only drives navigation and the rail/progress UI.
 */
export const WIZARD_STEPS: ReadonlyArray<{ id: WizardStep; label: string }> = [
  { id: "admin", label: "Admin" },
  { id: "network", label: "Network" },
  { id: "dhcp", label: "DHCP" },
  { id: "router_mac", label: "Router" },
  { id: "dns", label: "DNS" },
  { id: "tunnel", label: "Tunnel" },
  { id: "policy", label: "Policy" },
  { id: "remote_access", label: "HTTPS" },
  { id: "review", label: "Review" },
  { id: "completed", label: "Done" },
];

/**
 * Earliest step the wizard may rewind to. Admin creation is one-shot
 * (`POST /api/setup` returns 409 once an admin exists), so the daemon
 * rejects rewinds below this floor and the UI never offers them.
 */
export const REWIND_FLOOR: WizardStep = "network";

/** Position of `step` in the canonical order. */
export function ordinalOf(step: WizardStep): number {
  return WIZARD_STEPS.findIndex((s) => s.id === step);
}

/** The step after `step`, or `null` when `step` is terminal. */
export function nextOf(step: WizardStep): WizardStep | null {
  return WIZARD_STEPS[ordinalOf(step) + 1]?.id ?? null;
}

/** The step before `step`, or `null` at the head of the order. */
export function prevOf(step: WizardStep): WizardStep | null {
  return WIZARD_STEPS[ordinalOf(step) - 1]?.id ?? null;
}

/**
 * Whether the wizard may navigate from `current` back to `target`:
 * strictly backward, never below the floor, and never once completed.
 */
export function canRewindTo(current: WizardStep, target: WizardStep): boolean {
  if (current === "completed") return false;
  return (
    ordinalOf(target) < ordinalOf(current) &&
    ordinalOf(target) >= ordinalOf(REWIND_FLOOR)
  );
}
