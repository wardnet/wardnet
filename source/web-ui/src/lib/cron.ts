import cronstrue from "cronstrue";

/** Convert a cron expression to a human-readable description. */
export function cronToHuman(expr: string): string {
  try {
    return cronstrue.toString(expr, { use24HourTimeFormat: false });
  } catch {
    return expr;
  }
}
