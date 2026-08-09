import {
  BookOpen,
  Code,
  Database,
  Download,
  Settings,
  Route,
  Shield,
  Ban,
  Network,
  Globe,
  Save,
  Users,
  Terminal,
  Sparkles,
  Layers,
  Lock,
  Server,
  Radio,
  Smartphone,
  Bell,
  Trash2,
  type LucideProps,
} from "lucide-react";
import type { ComponentType } from "react";

const ICON_MAP: Record<string, ComponentType<LucideProps>> = {
  "book-open": BookOpen,
  code: Code,
  database: Database,
  download: Download,
  settings: Settings,
  route: Route,
  shield: Shield,
  ban: Ban,
  network: Network,
  globe: Globe,
  save: Save,
  users: Users,
  terminal: Terminal,
  sparkles: Sparkles,
  layers: Layers,
  lock: Lock,
  server: Server,
  radio: Radio,
  smartphone: Smartphone,
  bell: Bell,
  "trash-2": Trash2,
};

/** Resolves a string icon name from YAML content to a Lucide icon component. */
export function resolveIcon(name: string): ComponentType<LucideProps> | undefined {
  // eslint-disable-next-line security/detect-object-injection -- read-only lookup in a fixed ICON_MAP; name comes from build-time repo YAML, not user input
  return ICON_MAP[name];
}
