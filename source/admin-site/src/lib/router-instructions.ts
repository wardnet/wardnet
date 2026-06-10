/**
 * Per-router instructions for the setup wizard's DHCP onboarding step.
 *
 * The wizard renders the matching entry as a numbered checklist when the
 * operator picks their router model. The steps are intentionally
 * plain-string + optional kb_url instead of arbitrary markdown so we can
 * trust them at PR time (TypeScript schema review) without sandboxing a
 * markdown renderer.
 *
 * Adding a router is a one-entry PR: copy an existing object, fill in the
 * `id` (kebab-case), `name`, and `steps`, and link `kb_url` to the
 * official disable-DHCP guide if one exists.
 */
export interface RouterInstruction {
  /** Stable kebab-case ID used in API requests + URL segments. */
  id: string;
  /** Human-readable label shown in the picker dropdown. */
  name: string;
  /**
   * Numbered checklist shown to the operator. One step per array entry.
   * Keep entries short (one sentence each); the wizard renders them as
   * `<ol>` so prefixes like "1." are unnecessary.
   */
  steps: string[];
  /**
   * Link to the manufacturer's general support landing. Optional — many
   * ISP-supplied routers have no public KB and the embedded steps are
   * all the operator gets. We deliberately point at the support landing
   * rather than a deep DHCP article URL: vendors rotate those routinely
   * and a 404 in the wizard is worse than a one-extra-click landing.
   */
  kb_url?: string;
  /**
   * Optional asset path under `/public/router-images/` showing the
   * specific UI screen the operator should be looking at. Currently
   * unused by the v1 picker but reserved so we can add screenshots
   * without changing the schema.
   */
  image?: string;
}

/**
 * v1 router list. Order matters — the first entries are the most common
 * units in our target markets (UK + Kenya), with the manufacturer-brand
 * routers and the "Other" sentinel after them.
 */
export const ROUTER_INSTRUCTIONS: RouterInstruction[] = [
  {
    id: "safaricom-fiber-router",
    name: "Safaricom Fiber Router",
    steps: [
      "Open a browser on a device connected to the router and visit http://192.168.100.1.",
      "Sign in with the admin credentials printed on the router's underside.",
      'Open "Network" → "LAN" (or "DHCP Settings" depending on firmware).',
      'Toggle "DHCP Server" off and click "Apply" to save.',
      "Reboot the router so the change takes effect.",
    ],
  },
  {
    id: "vodafone-smart-hub",
    name: "Vodafone Smart Hub",
    steps: [
      "Browse to http://192.168.1.1 and sign in with the password on the router label.",
      'Go to "Advanced settings" → "Network" → "LAN".',
      'Set "DHCP" to "Disabled" and save.',
      "Reboot the hub.",
    ],
    kb_url: "https://www.vodafone.co.uk/help-and-information",
  },
  {
    id: "bt-smart-hub",
    name: "BT Smart Hub / Home Hub",
    steps: [
      "Browse to http://192.168.1.254 and sign in with the admin password on the hub.",
      'Go to "Advanced Settings" → "Home Network" → "IP Addresses".',
      'Switch "DHCP" to "Off" and apply.',
      "Reboot the hub.",
    ],
    kb_url: "https://www.bt.com/help/broadband/",
  },
  {
    id: "sky-hub",
    name: "Sky Hub / Sky Broadband Hub",
    steps: [
      "Browse to http://192.168.0.1 and sign in (default admin / sky).",
      'Go to "Maintenance" → "Router settings" → "LAN setup".',
      "Disable the DHCP server and save.",
      "Reboot the hub.",
    ],
    kb_url: "https://www.sky.com/help",
  },
  {
    id: "virgin-media-hub",
    name: "Virgin Media Hub",
    steps: [
      "Switch the hub to Modem mode (which disables the LAN-side DHCP server) via the Virgin Media app, or:",
      "Browse to http://192.168.0.1, sign in with the admin password on the hub.",
      'Open "Advanced Settings" → "Modem mode" and enable it.',
      "The hub reboots; from now on it stops handing out leases.",
    ],
    kb_url: "https://www.virginmedia.com/help",
  },
  {
    id: "asus",
    name: "ASUS router (AsusWRT)",
    steps: [
      "Browse to http://router.asus.com (or 192.168.1.1) and sign in.",
      'Go to "LAN" → "DHCP Server" tab.',
      'Set "Enable the DHCP Server" to "No" and click "Apply".',
    ],
    kb_url: "https://www.asus.com/support/",
  },
  {
    id: "netgear",
    name: "Netgear router",
    steps: [
      "Browse to http://www.routerlogin.net (or 192.168.1.1) and sign in.",
      'Go to "Advanced" → "Setup" → "LAN Setup".',
      'Untick "Use Router as DHCP Server" and apply.',
      "The router restarts to apply the change.",
    ],
    kb_url: "https://www.netgear.com/support/home/",
  },
  {
    id: "tp-link",
    name: "TP-Link router",
    steps: [
      "Browse to http://tplinkwifi.net (or 192.168.0.1) and sign in.",
      'Go to "Advanced" → "Network" → "DHCP Server".',
      'Set "DHCP Server" to "Disable" and save.',
      "Reboot the router.",
    ],
    kb_url: "https://www.tp-link.com/support/faq/",
  },
  {
    id: "ubiquiti",
    name: "Ubiquiti (UniFi / EdgeRouter)",
    steps: [
      "Sign in to the UniFi Network app (or EdgeOS web UI).",
      'For UniFi: open "Settings" → "Networks" → your LAN → "DHCP Mode" → "None".',
      'For EdgeRouter: "Services" → "DHCP Server" → toggle off the LAN pool.',
      "Save and apply changes.",
    ],
    kb_url: "https://help.ui.com/hc/en-us",
  },
  {
    id: "other",
    name: "Other / not listed",
    steps: [
      "Open your router's admin interface (usually 192.168.0.1, 192.168.1.1, or printed on the router label).",
      'Sign in with the admin credentials and find the "DHCP" or "LAN" settings.',
      "Disable the DHCP server / DHCP service and save.",
      "Reboot the router so the change takes effect.",
      "If you can't find the setting, switch to the locked-router branch on the previous screen — Wardnet works without owning DHCP, you just configure each device manually.",
    ],
  },
];

/**
 * Convenience lookup. Returns `undefined` for unknown IDs; callers
 * should fall back to the "other" entry.
 */
export function findRouterInstruction(
  id: string,
): RouterInstruction | undefined {
  return ROUTER_INSTRUCTIONS.find((r) => r.id === id);
}
