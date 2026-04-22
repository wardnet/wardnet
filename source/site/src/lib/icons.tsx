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
};

/** Resolves a string icon name from YAML content to a Lucide icon component. */
export function resolveIcon(name: string): ComponentType<LucideProps> | undefined {
  return ICON_MAP[name];
}
