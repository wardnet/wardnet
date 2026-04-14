/**
 * Shared mutable state across all test steps.
 *
 * Earlier steps populate IDs that later steps depend on.
 * Works because everything runs in a single process, sequentially.
 */
export const state = {
  tunnel1Id: "",
  tunnel2Id: "",
  alpineDeviceId: "",
  ubuntuDeviceId: "",
};
