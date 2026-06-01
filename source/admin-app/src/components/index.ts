// Reusable shell primitives — imported by D-slice screen components.
// Shell-internal components (Header, ConnectionBanner, TabBar) and
// feature-level components (InstallPrompt) are imported directly, not
// re-exported here.
export { ConfirmDialog } from "./ConfirmDialog";
export { BusyOverlay } from "./BusyOverlay";
export type { BusyPhase, BusyAction } from "./BusyOverlay";
