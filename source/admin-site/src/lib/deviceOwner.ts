/**
 * The value a "nobody" option carries in the device-owner `Select`.
 *
 * A sentinel exists at all because `Select` has no concept of a null option
 * value — it deals in strings.
 */
export const UNASSIGNED_OWNER = "__unassigned__";

/**
 * Translate the selected option back into what the API expects.
 *
 * Lives here rather than beside the component so it can be tested directly:
 * the dropdown is a Radix `Select`, which renders no native element and cannot
 * be driven in jsdom, so a DOM-level test could never reach this. Getting it
 * wrong is not cosmetic — sending the sentinel as an owner id would fail the
 * foreign key, and `""` would be a request to assign a user that does not
 * exist.
 */
export function ownerValueToId(selected: string): string | null {
  return selected === UNASSIGNED_OWNER ? null : selected;
}
